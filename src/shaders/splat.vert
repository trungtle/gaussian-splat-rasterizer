#version 450
// Projects one 3D Gaussian per instance into a screen-space quad aligned with the
// eigenvectors of its 2D covariance (EWA splatting, Zwicker et al. 2001 / Kerbl et al. 2023).

struct Splat {
    vec4 pos_opacity;
    vec4 cov_a; // xx xy xz yy
    vec4 cov_b; // yz zz - -
};

layout(set = 0, binding = 0, std430) readonly buffer Splats { Splat splats[]; };
layout(set = 0, binding = 1, std430) readonly buffer Sh { float sh[]; };
layout(set = 0, binding = 2, std430) readonly buffer Order { uint order[]; };

layout(push_constant) uniform Push {
    mat4 view;        // world -> camera (x right, y down, z forward)
    vec4 cam_pos;     // xyz = camera position, w = sh degree
    vec4 intrinsics;  // fx, fy, width, height
} pc;

layout(location = 0) out vec4 v_color;
layout(location = 1) out vec2 v_uv;

const float SH_C0 = 0.28209479177387814;
const float SH_C1 = 0.4886025119029199;
const float SH_C2[5] = float[](1.0925484305920792, -1.0925484305920792, 0.31539156525252005,
                               -1.0925484305920792, 0.5462742152960396);
const float SH_C3[7] = float[](-0.5900435899266435, 2.890611442640554, -0.4570457994644658,
                               0.3731763325901154, -0.4570457994644658, 1.445305721320277,
                               -0.5900435899266435);

vec3 shc(uint base, int k) {
    uint o = base + uint(k) * 3u;
    return vec3(sh[o], sh[o + 1u], sh[o + 2u]);
}

vec3 eval_sh(uint idx, vec3 dir, int degree) {
    uint base = idx * 48u;
    vec3 c = SH_C0 * shc(base, 0);
    if (degree > 0) {
        float x = dir.x, y = dir.y, z = dir.z;
        c += SH_C1 * (-y * shc(base, 1) + z * shc(base, 2) - x * shc(base, 3));
        if (degree > 1) {
            float xx = x * x, yy = y * y, zz = z * z, xy = x * y, yz = y * z, xz = x * z;
            c += SH_C2[0] * xy * shc(base, 4) + SH_C2[1] * yz * shc(base, 5)
               + SH_C2[2] * (2.0 * zz - xx - yy) * shc(base, 6)
               + SH_C2[3] * xz * shc(base, 7) + SH_C2[4] * (xx - yy) * shc(base, 8);
            if (degree > 2) {
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
    return max(c + 0.5, vec3(0.0));
}

const vec2 CORNERS[6] = vec2[](vec2(-1, -1), vec2(1, -1), vec2(1, 1),
                               vec2(-1, -1), vec2(1, 1), vec2(-1, 1));

void cull() {
    gl_Position = vec4(0.0, 0.0, 2.0, 1.0); // outside the depth range -> clipped
    v_color = vec4(0.0);
    v_uv = vec2(0.0);
}

void main() {
    uint idx = order[gl_InstanceIndex];
    Splat s = splats[idx];
    vec3 t = (pc.view * vec4(s.pos_opacity.xyz, 1.0)).xyz;
    if (t.z < 0.2) { cull(); return; }

    float fx = pc.intrinsics.x, fy = pc.intrinsics.y;
    vec2 size = pc.intrinsics.zw;

    // Clamp the Jacobian's evaluation point to slightly beyond the frustum, as in the reference.
    vec2 lim = 1.3 * 0.5 * size / vec2(fx, fy);
    vec2 txy = clamp(t.xy / t.z, -lim, lim) * t.z;
    mat3 J = mat3(fx / t.z, 0.0, 0.0,
                  0.0, fy / t.z, 0.0,
                  -fx * txy.x / (t.z * t.z), -fy * txy.y / (t.z * t.z), 0.0);
    mat3 W = mat3(pc.view);
    mat3 T = J * W;
    mat3 V = mat3(s.cov_a.x, s.cov_a.y, s.cov_a.z,
                  s.cov_a.y, s.cov_a.w, s.cov_b.x,
                  s.cov_a.z, s.cov_b.x, s.cov_b.y);
    mat3 cov = T * V * transpose(T);
    // Low-pass filter: every splat covers at least ~one pixel.
    float a = cov[0][0] + 0.3, b = cov[0][1], c = cov[1][1] + 0.3;

    float mid = 0.5 * (a + c);
    float rad = length(vec2(0.5 * (a - c), b));
    float l1 = mid + rad, l2 = max(mid - rad, 0.0);
    if (l2 <= 0.0) { cull(); return; }
    vec2 e1 = (abs(b) > 1e-8) ? normalize(vec2(b, l1 - a)) : (a >= c ? vec2(1, 0) : vec2(0, 1));
    vec2 e2 = vec2(-e1.y, e1.x);
    // 3 sigma extent, capped to keep huge near-camera splats from flooding fill rate.
    vec2 ext = 3.0 * sqrt(vec2(l1, l2));
    ext = min(ext, vec2(2048.0));

    vec2 center_px = vec2(fx * t.x / t.z, fy * t.y / t.z); // relative to image center
    vec2 half_size = 0.5 * size;
    // Frustum cull on the conservative bounding circle.
    if (any(greaterThan(abs(center_px) - ext.x, half_size))) { cull(); return; }

    vec2 corner = CORNERS[gl_VertexIndex];
    vec2 offset_px = corner.x * ext.x * e1 + corner.y * ext.y * e2;
    vec2 ndc = (center_px + offset_px) / half_size;

    gl_Position = vec4(ndc, 0.5, 1.0);
    vec3 dir = normalize(s.pos_opacity.xyz - pc.cam_pos.xyz);
    v_color = vec4(eval_sh(idx, dir, int(pc.cam_pos.w)), s.pos_opacity.w);
    v_uv = corner * 3.0; // in units of standard deviations along each eigen-axis
}
