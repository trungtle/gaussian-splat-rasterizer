#version 450

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec2 v_uv;
layout(location = 0) out vec4 f_color;

void main() {
    float power = -0.5 * dot(v_uv, v_uv);
    float alpha = min(0.99, v_color.a * exp(power));
    if (alpha < 1.0 / 255.0) discard;
    // Premultiplied output; blended back-to-front with (ONE, ONE_MINUS_SRC_ALPHA).
    f_color = vec4(v_color.rgb * alpha, alpha);
}
