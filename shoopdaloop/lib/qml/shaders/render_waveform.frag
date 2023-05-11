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

// Waveform is stored in 2D texture such that maximum use can be made of available
// GL_MAX_TEXTURE_SIZE.
layout(binding = 1) uniform sampler2D waveform;

float get_waveform_sample(int sample_idx) {
    vec2 waveform_size = textureSize(waveform, 0);
    vec2 waveform_coord_pixels = vec2(
        sample_idx % int(waveform_size.x),
        sample_idx / waveform_size.x
    );
    vec2 waveform_coord = waveform_coord_pixels / waveform_size;
    // Recombine input pixel to a 32-bit unsigned value representing the waveform sample.
    vec4 in_pix = texture(waveform, waveform_coord);
    uint a = uint(in_pix.a * 255.0) << 24;
    uint r = uint(in_pix.r * 255.0) << 16;
    uint g = uint(in_pix.g * 255.0) << 8;
    uint b = uint(in_pix.b * 255.0);
    float samp = float(a | r | g | b) / float(pow(2, 32)) * 2.0 - 1.0;
    return samp;
}

void main() {
    int total_offset = pixels_offset + (samples_offset / samples_per_pixel);
    float samp = get_waveform_sample(int(gl_FragCoord.x + total_offset));
    
    // Render as a waveform, extending up and down from center.
    float lower_thres = 0.5 - (samp/2.0);
    float upper_thres = 0.5 + (samp/2.0);
    bool is_on = (qt_TexCoord0.y >= lower_thres && qt_TexCoord0.y <= upper_thres);
    fragColor = vec4(is_on ? 1.0 : 0.0, 0.0, 0.0, 1.0) * qt_Opacity;
}