# version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    int samples_per_pixel;
    int samples_offset;
    int pixels_offset;
};

// Audio is stored in 2D texture such that maximum use can be made of available
// GL_MAX_TEXTURE_SIZE.
layout(binding = 1) uniform sampler2D audio;

float get_audio_sample(int sample_idx) {
    vec2 audio_size = textureSize(audio, 0);
    vec2 audio_coord_pixels = vec2(
        sample_idx % int(audio_size.x),
        sample_idx / audio_size.x
    );
    vec2 audio_coord = audio_coord_pixels / audio_size;
    // Recombine input pixel to a 32-bit unsigned value representing the audio sample.
    vec4 in_pix = texture(audio, audio_coord);
    uint a = uint(in_pix.a * 255.0) << 24;
    uint r = uint(in_pix.r * 255.0) << 16;
    uint g = uint(in_pix.g * 255.0) << 8;
    uint b = uint(in_pix.b * 255.0);
    float samp = float(a | r | g | b) / float(pow(2, 32)) * 2.0 - 1.0;
    return samp;
}

void main() {
    int total_offset = pixels_offset + (samples_offset / samples_per_pixel);
    float samp = get_audio_sample(int(gl_FragCoord.x + total_offset));

    // Do RMS + dB calculation
    // float db = 20.0 * log(abs(samp)) / log(10.0);
    
    // Render as a waveform, extending up and down from center.
    float db_in_0_1_range = samp; //max(1.0 - (-db) / 45.0, 0.0);
    float lower_thres = 0.5 - (db_in_0_1_range/2.0);
    float upper_thres = 0.5 + (db_in_0_1_range/2.0);
    bool is_on = (qt_TexCoord0.y >= lower_thres && qt_TexCoord0.y <= upper_thres);
    fragColor = vec4(is_on ? 1.0 : 0.0, 0.0, 0.0, 1.0) * qt_Opacity;
}