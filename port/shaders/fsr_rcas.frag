#version 330 core
// AMD FidelityFX Super Resolution 1.0 — RCAS (robust contrast-adaptive sharpening).
// GLSL port of FsrRcasF from ffx_fsr1.h (MIT, see LICENSE-FSR.txt), without the optional denoise.
// Reads the EASU output (row 0 = top) and writes the letterbox rect of the framebuffer (bottom-up).
uniform sampler2D src;   // EASU output, same size as the rect
uniform ivec2 origin;    // rect origin in framebuffer pixels (bottom-left)
uniform float sharp;     // exp2(-stops): 1.0 = strongest
out vec4 frag;

#define FSR_RCAS_LIMIT (0.25 - (1.0 / 16.0))

vec3 tap(ivec2 p) {
    return texelFetch(src, clamp(p, ivec2(0), textureSize(src, 0) - 1), 0).rgb;
}

void main() {
    ivec2 q = ivec2(gl_FragCoord.xy) - origin;
    ivec2 sp = ivec2(q.x, textureSize(src, 0).y - 1 - q.y);
    //    b
    //  d e f
    //    h
    vec3 b = tap(sp + ivec2(0, -1));
    vec3 d = tap(sp + ivec2(-1, 0));
    vec3 e = tap(sp);
    vec3 f = tap(sp + ivec2(1, 0));
    vec3 h = tap(sp + ivec2(0, 1));
    // min and max of the ring
    vec3 mn4 = min(min(b, d), min(f, h));
    vec3 mx4 = max(max(b, d), max(f, h));
    // limiters (guarded against the 0/0 the GPU rcp approximations hide)
    vec2 peakC = vec2(1.0, -4.0);
    vec3 hitMin = min(mn4, e) / max(4.0 * mx4, vec3(1.0 / 65536.0));
    vec3 hitMax = (peakC.x - max(mx4, e)) / min(4.0 * mn4 + peakC.y, vec3(-1.0 / 65536.0));
    vec3 lobeRGB = max(-hitMin, hitMax);
    float lobe = max(-FSR_RCAS_LIMIT, min(max(lobeRGB.r, max(lobeRGB.g, lobeRGB.b)), 0.0)) * sharp;
    // resolve
    vec3 c = (lobe * (b + d + h + f) + e) / (4.0 * lobe + 1.0);
    frag = vec4(clamp(c, 0.0, 1.0), 1.0);
}
