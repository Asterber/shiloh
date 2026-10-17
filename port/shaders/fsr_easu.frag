#version 330 core
// AMD FidelityFX Super Resolution 1.0 — EASU (edge-adaptive spatial upsampling).
// GLSL port of FsrEasuF / FsrEasuSetF / FsrEasuTapF from ffx_fsr1.h (MIT, see LICENSE-FSR.txt).
// 12 taps by texelFetch instead of gathers; the approximate rcp/rsqrt of the original are exact ones here.
// Renders the whole intermediate target; output row 0 = top (same orientation as the source).
uniform sampler2D src;   // the 640x480 picture, row 0 = top
uniform vec4 con0;       // xy = input/output size, zw = 0.5 * input/output - 0.5
out vec4 frag;

vec3 tap(ivec2 p) {
    return texelFetch(src, clamp(p, ivec2(0), textureSize(src, 0) - 1), 0).rgb;
}

// luma times 2
float luma(vec3 c) { return c.b * 0.5 + (c.r * 0.5 + c.g); }

//    a
//  b c d
//    e
void setF(inout vec2 dir, inout float len, float w, float lA, float lB, float lC, float lD, float lE) {
    float dc = lD - lC;
    float cb = lC - lB;
    float lenX = 1.0 / max(max(abs(dc), abs(cb)), 1.0e-5);
    float dirX = lD - lB;
    dir.x += dirX * w;
    lenX = clamp(abs(dirX) * lenX, 0.0, 1.0);
    lenX *= lenX;
    len += lenX * w;
    float ec = lE - lC;
    float ca = lC - lA;
    float lenY = 1.0 / max(max(abs(ec), abs(ca)), 1.0e-5);
    float dirY = lE - lA;
    dir.y += dirY * w;
    lenY = clamp(abs(dirY) * lenY, 0.0, 1.0);
    lenY *= lenY;
    len += lenY * w;
}

void tapF(inout vec3 aC, inout float aW, vec2 off, vec2 dir, vec2 len, float lob, float clp, vec3 c) {
    // rotate the offset by the direction, anisotropy
    vec2 v = vec2(off.x * dir.x + off.y * dir.y, off.x * -dir.y + off.y * dir.x) * len;
    // distance^2, limited to the window
    float d2 = min(v.x * v.x + v.y * v.y, clp);
    // lanczos2 approximation: (25/16 * (2/5 * x^2 - 1)^2 - (25/16 - 1)) * (lob * x^2 - 1)^2
    float wB = 2.0 / 5.0 * d2 - 1.0;
    float wA = lob * d2 - 1.0;
    wB *= wB;
    wA *= wA;
    wB = 25.0 / 16.0 * wB - (25.0 / 16.0 - 1.0);
    float w = wB * wA;
    aC += c * w;
    aW += w;
}

void main() {
    vec2 pp = floor(gl_FragCoord.xy) * con0.xy + con0.zw;
    vec2 fp = floor(pp);
    pp -= fp;
    ivec2 f = ivec2(fp);
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    vec3 bC = tap(f + ivec2(0, -1)), cC = tap(f + ivec2(1, -1));
    vec3 eC = tap(f + ivec2(-1, 0)), fC = tap(f), gC = tap(f + ivec2(1, 0)), hC = tap(f + ivec2(2, 0));
    vec3 iC = tap(f + ivec2(-1, 1)), jC = tap(f + ivec2(0, 1)), kC = tap(f + ivec2(1, 1)), lC = tap(f + ivec2(2, 1));
    vec3 nC = tap(f + ivec2(0, 2)), oC = tap(f + ivec2(1, 2));
    float bL = luma(bC), cL = luma(cC), eL = luma(eC), fL = luma(fC), gL = luma(gC), hL = luma(hC);
    float iL = luma(iC), jL = luma(jC), kL = luma(kC), lL = luma(lC), nL = luma(nC), oL = luma(oC);

    // direction and length, bilinearly weighted over the 4 nearest quads (s t / u v)
    vec2 dir = vec2(0.0);
    float len = 0.0;
    setF(dir, len, (1.0 - pp.x) * (1.0 - pp.y), bL, eL, fL, gL, jL);
    setF(dir, len, pp.x * (1.0 - pp.y), cL, fL, gL, hL, kL);
    setF(dir, len, (1.0 - pp.x) * pp.y, fL, iL, jL, kL, nL);
    setF(dir, len, pp.x * pp.y, gL, jL, kL, lL, oL);

    // normalize the direction, defaulting to (1, 0) when it is too small
    vec2 dir2 = dir * dir;
    float dirR = dir2.x + dir2.y;
    bool zro = dirR < 1.0 / 32768.0;
    dirR = zro ? 1.0 : inversesqrt(dirR);
    dir.x = zro ? 1.0 : dir.x;
    dir *= dirR;
    // {0 to 2} -> {0 to 1}, shaped with a square
    len = len * 0.5;
    len *= len;
    // stretch kernel {1.0 vert|horz, to sqrt(2.0) on diagonal}
    float stretch = (dir.x * dir.x + dir.y * dir.y) / max(abs(dir.x), abs(dir.y));
    vec2 len2 = vec2(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
    // the window shifts from +/-{sqrt(2.0) to slightly beyond 2.0} with the amount of edge
    float lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
    float clp = 1.0 / lob;

    // deringing: clamp to the min / max of the 4 nearest
    vec3 min4 = min(min(fC, gC), min(jC, kC));
    vec3 max4 = max(max(fC, gC), max(jC, kC));

    vec3 aC = vec3(0.0);
    float aW = 0.0;
    tapF(aC, aW, vec2(0.0, -1.0) - pp, dir, len2, lob, clp, bC);
    tapF(aC, aW, vec2(1.0, -1.0) - pp, dir, len2, lob, clp, cC);
    tapF(aC, aW, vec2(-1.0, 1.0) - pp, dir, len2, lob, clp, iC);
    tapF(aC, aW, vec2(0.0, 1.0) - pp, dir, len2, lob, clp, jC);
    tapF(aC, aW, vec2(0.0, 0.0) - pp, dir, len2, lob, clp, fC);
    tapF(aC, aW, vec2(-1.0, 0.0) - pp, dir, len2, lob, clp, eC);
    tapF(aC, aW, vec2(1.0, 1.0) - pp, dir, len2, lob, clp, kC);
    tapF(aC, aW, vec2(2.0, 1.0) - pp, dir, len2, lob, clp, lC);
    tapF(aC, aW, vec2(2.0, 0.0) - pp, dir, len2, lob, clp, hC);
    tapF(aC, aW, vec2(1.0, 0.0) - pp, dir, len2, lob, clp, gC);
    tapF(aC, aW, vec2(1.0, 2.0) - pp, dir, len2, lob, clp, oC);
    tapF(aC, aW, vec2(0.0, 2.0) - pp, dir, len2, lob, clp, nC);
    frag = vec4(min(max4, max(min4, aC / aW)), 1.0);
}
