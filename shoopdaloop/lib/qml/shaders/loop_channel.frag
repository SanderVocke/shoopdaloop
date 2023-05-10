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
    vec2 audio_coord_pixels = vec2(
        int(gl_FragCoord.x) % int(audio_size.x),
        int(gl_FragCoord.y / audio_size.x)
    );
    vec2 audio_coord = audio_coord_pixels / audio_size;
    vec4 samp = texture(audio, audio_coord);
    float power = sqrt(samp.x*samp.x);
    
    // Render as a waveform, extending up and down from center.
    float lower_thres = 0.5 - (power/2.0);
    float upper_thres = 0.5 + (power/2.0);
    bool is_on = (qt_TexCoord0.y >= lower_thres && qt_TexCoord0.y <= upper_thres);
    fragColor = vec4(is_on ? 1.0 : 0.0, 0.0, 0.0, 1.0) * qt_Opacity;

    //fragColor = texture(audio, audio_coord) * qt_Opacity;
    //fragColor = vec4(qt_TexCoord0.x, qt_TexCoord0.y, 0.0, 1.0) * qt_Opacity;
}