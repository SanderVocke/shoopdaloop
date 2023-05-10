# version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
};

// Audio is stored in 2D texture such that maximum use can be made of available
// GL_MAX_TEXTURE_SIZE.
layout(binding = 1) uniform sampler2D audio;
void main() {
    vec2 audio_size = textureSize(audio, 0);
    vec2 audio_coord = vec2(
        int(qt_TexCoord0.x) % int(audio_size.x),
        int(qt_TexCoord0.y / audio_size.x)
    );
    fragColor = texture(audio, audio_coord) * qt_Opacity;
}