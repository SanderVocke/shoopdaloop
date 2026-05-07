#ifndef BASE64_WRAPPER_HPP
#define BASE64_WRAPPER_HPP

extern "C" {
#include <b64/cencode.h>
#include <b64/cdecode.h>
}
#include <string>
#include <vector>

namespace base64 {

inline std::string to_base64(std::string const &data) {
    base64_encodestate state;
    base64_init_encodestate(&state);
    
    // Calculate output size: input * 4/3 + padding + null terminator
    size_t max_len = ((data.size() + 2) / 3) * 4 + 1;
    std::vector<char> encoded(max_len);
    
    size_t len = base64_encode_block(data.data(), data.size(), encoded.data(), &state);
    len += base64_encode_blockend(encoded.data() + len, &state);
    
    return std::string(encoded.data(), len);
}

inline std::string from_base64(std::string const &data) {
    base64_decodestate state;
    base64_init_decodestate(&state);
    
    // Calculate output size: input * 3/4
    size_t max_len = (data.size() / 4) * 3 + 1;
    std::vector<char> decoded(max_len);
    
    size_t len = base64_decode_block(data.data(), data.size(), decoded.data(), &state);
    
    return std::string(decoded.data(), len);
}

} // namespace base64

#endif // BASE64_WRAPPER_HPP