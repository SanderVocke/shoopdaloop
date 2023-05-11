# version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    int samples_per_pixel;
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

vec4 to_32bit_argb(float s) {
    uint bits = uint(s * 4294967295.0); // 32-bit max
    return vec4(
        (0xFF & (bits >> 16)) / 255.0,
        (0xFF & (bits >> 8 )) / 255.0,
        (0xFF & (bits      )) / 255.0,
        (0xFF & (bits >> 24)) / 255.0
    );
}

void main() {
    // Calculate RMS
    vec2 texsize = textureSize(waveform, 0);
    vec2 pixsize = vec2(1.0, 1.0) / texsize;
    int pos = int( (gl_FragCoord.x / pixsize.x) +
                   (gl_FragCoord.y / pixsize.y) * texsize.x );
    int start = int(pos * samples_per_pixel);
    float v = 0;
    for(int idx=start; idx<(start + samples_per_pixel); idx++) {
        float s = get_waveform_sample(idx);
        v += s*s;
    }
    v = sqrt(v / samples_per_pixel);

    // Do dB calculation
    float db = 20.0 * log(v) / log(10.0);
    float db_in_0_1_range = max(1.0 - (-db) / 45.0, 0.0);
    
    // Convert back to color
    // fragColor = to_32bit_argb(db_in_0_1_range);
    fragColor = to_32bit_argb(get_waveform_sample(start));
}