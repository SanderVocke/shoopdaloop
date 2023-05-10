# version 440

layout(location = 0) in vec2 coord;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
};
void main() {
    fragColor = vec4(1.0, 1.0, 1.0, 1.0) * qt_Opacity;
}