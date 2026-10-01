// 3D Gaussian splatting: one instance per Gaussian, expanded into a screen-space
// quad aligned with the eigenvectors of its projected 2D covariance
// (EWA splatting, Zwicker et al. 2001 / Kerbl et al. 2023).
//
// Entry points: vsMain (vertex), fsMain (fragment). Compiled by build.rs with
// -matrix-layout-column-major so float4x4 matches glam's column-major layout.

struct Splat
{
    float4 pos_opacity; // xyz world position, w opacity (post-sigmoid)
    float4 cov_a;       // xx xy xz yy
    float4 cov_b;       // yz zz - -
};

[[vk::binding(0, 0)]] StructuredBuffer<Splat> splats;
[[vk::binding(1, 0)]] StructuredBuffer<float> sh;    // 48 floats per splat, [coeff][rgb]
[[vk::binding(2, 0)]] StructuredBuffer<uint> order;  // back-to-front splat indices

// Every member is 16-byte aligned so std140 and std430 layouts agree.
// Must match `Push` in src/renderer.rs.
struct Push
{
    float4x4 view;     // world -> camera (x right, y down, z forward)
    float4 cam_pos;    // xyz camera position, w = SH degree to evaluate
    float4 intrinsics; // fx, fy, width, height
    int4 dbg;          // x = view mode (src/debug.rs ViewMode order)
    float4 params;     // x = splat scale, y = min opacity, z = depth near, w = depth far
};
[[vk::push_constant]] ConstantBuffer<Push> pc;

// View modes; order matches ViewMode::ALL in src/debug.rs.
static const int MODE_COLOR = 0;
static const int MODE_BASE_COLOR = 1;
static const int MODE_VIEW_DEPENDENT = 2;
static const int MODE_DEPTH = 3;
static const int MODE_OPACITY = 4;
static const int MODE_ELLIPSES = 5;
static const int MODE_CENTERS = 6;
static const int MODE_ANISOTROPY = 7;
static const int MODE_OVERDRAW = 8;
static const int MODE_QUADS = 9;

struct VSOut
{
    float4 pos : SV_Position;
    float4 color : COLOR;                      // rgb, opacity
    float2 uv : TEXCOORD0;                     // offset in standard deviations along the eigen-axes
    nointerpolation float4 extra : TEXCOORD1;  // x depth01, y opacity, z anisotropy01
    nointerpolation int mode : TEXCOORD2;
};

static const float SH_C0 = 0.28209479177387814;
static const float SH_C1 = 0.4886025119029199;
static const float SH_C2[5] = { 1.0925484305920792, -1.0925484305920792, 0.31539156525252005,
                                -1.0925484305920792, 0.5462742152960396 };
static const float SH_C3[7] = { -0.5900435899266435, 2.890611442640554, -0.4570457994644658,
                                0.3731763325901154, -0.4570457994644658, 1.445305721320277,
                                -0.5900435899266435 };

float3 shc(uint base, int k)
{
    uint o = base + uint(k) * 3;
    return float3(sh[o], sh[o + 1], sh[o + 2]);
}

// Evaluate the SH color. `bandStart` = 0 for the full color, 1 to drop the
// view-independent DC term (debug view of the view-dependent part only).
float3 evalSH(uint idx, float3 dir, int degree, int bandStart)
{
    uint base = idx * 48;
    float3 c = bandStart == 0 ? SH_C0 * shc(base, 0) : float3(0.0);
    if (degree > 0)
    {
        float x = dir.x, y = dir.y, z = dir.z;
        c += SH_C1 * (-y * shc(base, 1) + z * shc(base, 2) - x * shc(base, 3));
        if (degree > 1)
        {
            float xx = x * x, yy = y * y, zz = z * z, xy = x * y, yz = y * z, xz = x * z;
            c += SH_C2[0] * xy * shc(base, 4) + SH_C2[1] * yz * shc(base, 5)
               + SH_C2[2] * (2.0 * zz - xx - yy) * shc(base, 6)
               + SH_C2[3] * xz * shc(base, 7) + SH_C2[4] * (xx - yy) * shc(base, 8);
            if (degree > 2)
            {
                c += SH_C3[0] * y * (3.0 * xx - yy) * shc(base, 9)
                   + SH_C3[1] * xy * z * shc(base, 10)
                   + SH_C3[2] * y * (4.0 * zz - xx - yy) * shc(base, 11)
                   + SH_C3[3] * z * (2.0 * zz - 3.0 * xx - 3.0 * yy) * shc(base, 12)
                   + SH_C3[4] * x * (4.0 * zz - xx - yy) * shc(base, 13)
                   + SH_C3[5] * z * (xx - yy) * shc(base, 14)
                   + SH_C3[6] * x * (xx - 3.0 * yy) * shc(base, 15);
            }
        }
    }
    return max(c + 0.5, float3(0.0));
}

// Polynomial fit of Google's Turbo colormap (Mikhailov 2019): 0 = blue, 1 = red.
float3 turbo(float x)
{
    x = saturate(x);
    const float4 kR4 = float4(0.13572138, 4.61539260, -42.66032258, 132.13108234);
    const float4 kG4 = float4(0.09140261, 2.19418839, 4.84296658, -14.18503333);
    const float4 kB4 = float4(0.10667330, 12.64194608, -60.58204836, 110.36276771);
    const float2 kR2 = float2(-152.94239396, 59.28637943);
    const float2 kG2 = float2(4.27729857, 2.82956604);
    const float2 kB2 = float2(-89.90310912, 27.34824973);
    float4 v4 = float4(1.0, x, x * x, x * x * x);
    float2 v2 = v4.zw * v4.z;
    return float3(dot(v4, kR4) + dot(v2, kR2), dot(v4, kG4) + dot(v2, kG2), dot(v4, kB4) + dot(v2, kB2));
}

static const float2 CORNERS[6] = { float2(-1, -1), float2(1, -1), float2(1, 1),
                                   float2(-1, -1), float2(1, 1), float2(-1, 1) };

VSOut culled()
{
    VSOut o;
    o.pos = float4(0.0, 0.0, 2.0, 1.0); // outside the depth range -> clipped
    o.color = float4(0.0);
    o.uv = float2(0.0);
    o.extra = float4(0.0);
    o.mode = 0;
    return o;
}

[shader("vertex")]
VSOut vsMain(uint vertexId : SV_VulkanVertexID, uint instanceId : SV_VulkanInstanceID)
{
    int mode = pc.dbg.x;
    uint idx = order[instanceId];
    Splat s = splats[idx];
    float opacity = s.pos_opacity.w;
    if (opacity < pc.params.y)
        return culled();

    float3 t = mul(pc.view, float4(s.pos_opacity.xyz, 1.0)).xyz;
    if (t.z < 0.2)
        return culled();

    float fx = pc.intrinsics.x, fy = pc.intrinsics.y;
    float2 size = pc.intrinsics.zw;
    float2 halfSize = 0.5 * size;

    // Clamp the Jacobian's evaluation point to slightly beyond the frustum, as in the reference.
    float2 lim = 1.3 * halfSize / float2(fx, fy);
    float2 txy = clamp(t.xy / t.z, -lim, lim) * t.z;
    float3x3 J = float3x3(fx / t.z, 0.0, -fx * txy.x / (t.z * t.z),
                          0.0, fy / t.z, -fy * txy.y / (t.z * t.z),
                          0.0, 0.0, 0.0);
    float3x3 W = (float3x3)pc.view;
    float3x3 T = mul(J, W);
    float scale2 = pc.params.x * pc.params.x;
    float3x3 V = float3x3(s.cov_a.x, s.cov_a.y, s.cov_a.z,
                          s.cov_a.y, s.cov_a.w, s.cov_b.x,
                          s.cov_a.z, s.cov_b.x, s.cov_b.y) * scale2;
    float3x3 cov = mul(mul(T, V), transpose(T));
    // Low-pass filter: every splat covers at least ~one pixel.
    float a = cov[0][0] + 0.3, b = cov[0][1], c = cov[1][1] + 0.3;

    float mid = 0.5 * (a + c);
    float rad = length(float2(0.5 * (a - c), b));
    float l1 = mid + rad, l2 = max(mid - rad, 0.0);
    if (l2 <= 0.0)
        return culled();
    float2 e1 = abs(b) > 1e-8 ? normalize(float2(b, l1 - a)) : (a >= c ? float2(1, 0) : float2(0, 1));
    float2 e2 = float2(-e1.y, e1.x);
    // 3 sigma extent, capped to keep huge near-camera splats from flooding fill rate.
    float2 ext = min(3.0 * sqrt(float2(l1, l2)), float2(2048.0));
    if (mode == MODE_CENTERS)
    {
        ext = float2(2.5);
        e1 = float2(1, 0);
        e2 = float2(0, 1);
    }

    float2 centerPx = float2(fx * t.x / t.z, fy * t.y / t.z); // relative to image center
    // Frustum cull on the conservative bounding circle.
    if (any(abs(centerPx) - ext.x > halfSize))
        return culled();

    float2 corner = CORNERS[vertexId];
    float2 offsetPx = corner.x * ext.x * e1 + corner.y * ext.y * e2;

    VSOut o;
    o.pos = float4((centerPx + offsetPx) / halfSize, 0.5, 1.0);
    float3 dir = normalize(s.pos_opacity.xyz - pc.cam_pos.xyz);
    int degree = int(pc.cam_pos.w);
    float3 rgb;
    if (mode == MODE_BASE_COLOR)
        rgb = evalSH(idx, dir, 0, 0);
    else if (mode == MODE_VIEW_DEPENDENT)
        rgb = evalSH(idx, dir, degree, 1);
    else
        rgb = evalSH(idx, dir, degree, 0);
    o.color = float4(rgb, opacity);
    o.uv = corner * 3.0;
    float near = pc.params.z, far = pc.params.w;
    float depth01 = saturate(log(max(t.z, near) / near) / log(far / near));
    float aniso01 = saturate(log(sqrt(l1 / l2)) / log(10.0)); // 1 at 10:1 axis ratio
    o.extra = float4(depth01, opacity, aniso01, 0.0);
    o.mode = mode;
    return o;
}

[shader("fragment")]
float4 fsMain(VSOut i) : SV_Target
{
    float r2 = dot(i.uv, i.uv);
    float falloff = exp(-0.5 * r2);
    float alpha = min(0.99, i.color.a * falloff);
    float3 rgb = i.color.rgb;

    switch (i.mode)
    {
    case MODE_DEPTH:
        rgb = turbo(1.0 - i.extra.x); // near = red, far = blue
        break;
    case MODE_OPACITY:
        // Unit opacity so low-opacity splats stay visible; color carries the value.
        rgb = turbo(i.extra.y);
        alpha = min(0.99, falloff);
        break;
    case MODE_ANISOTROPY:
        rgb = turbo(i.extra.z);
        break;
    case MODE_ELLIPSES:
    {
        float r = sqrt(r2);
        float w = max(fwidth(r), 1e-4);
        float ring = saturate(max(1.0 - abs(r - 1.0) / w, 1.0 - abs(r - 2.0) / w));
        float fill = r < 2.0 ? 0.06 : 0.0;
        alpha = max(ring * max(i.color.a, 0.35), fill);
        break;
    }
    case MODE_CENTERS:
        if (r2 > 9.0)
            discard;
        return float4(rgb, 1.0);
    case MODE_OVERDRAW:
        if (alpha < 1.0 / 255.0)
            discard;
        // Additive blend: ~48 layers saturate red, then orange, then white.
        return float4(1.0 / 48.0, 1.0 / 160.0, 1.0 / 600.0, 0.0);
    case MODE_QUADS:
    {
        float e = max(abs(i.uv.x), abs(i.uv.y)) / 3.0;
        float w = max(fwidth(e), 1e-4);
        float edge = saturate(1.0 - (1.0 - e) / (1.5 * w));
        alpha = max(edge * 0.8, 0.03);
        break;
    }
    default:
        break;
    }

    if (alpha < 1.0 / 255.0)
        discard;
    // Premultiplied output; blended back-to-front with (ONE, ONE_MINUS_SRC_ALPHA).
    return float4(rgb * alpha, alpha);
}
