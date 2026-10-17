#version 330 core
// plain scaling of the picture into the letterbox rect (the texture filter decides linear / nearest)
uniform sampler2D src;   // the 640x480 picture, row 0 = top
uniform vec2 origin;     // rect origin in framebuffer pixels (bottom-left)
uniform vec2 size;       // rect size in framebuffer pixels
out vec4 frag;
void main() {
    vec2 uv = (gl_FragCoord.xy - origin) / size;
    uv.y = 1.0 - uv.y;
    frag = vec4(texture(src, uv).rgb, 1.0);
}
