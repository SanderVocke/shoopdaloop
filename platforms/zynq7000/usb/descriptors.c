#include "descriptors.h"

static unsigned word(const uint8_t *bytes) { return bytes[0] | (unsigned)bytes[1] << 8; }

bool shoop_usb_configuration_valid(const uint8_t *bytes, size_t length) {
    if (!bytes || length < 9 || length > 4096 || bytes[0] != 9 || bytes[1] != 2 ||
        word(bytes + 2) != length || !bytes[4] || bytes[4] > 32 || !bytes[5]) return false;
    bool midi = false, header = false, interface = false, default_alternate = false;
    uint32_t interfaces = 0, endpoint_addresses = 0;
    unsigned interface_count = 0;
    unsigned expected = 0, endpoints = 0, directions = 0;
    for (size_t offset = 9; offset < length;) {
        const uint8_t *desc = bytes + offset;
        size_t remaining = length - offset;
        if (remaining < 2 || desc[0] < 2 || desc[0] > remaining) return false;
        unsigned size = desc[0], type = desc[1];
        if (type == 4) {
            if (midi && (!header || endpoints != expected)) return false;
            if (size < 9 || desc[2] >= 32) return false;
            interface = true;
            default_alternate = desc[3] == 0;
            if (default_alternate) {
                if (interfaces & (1u << desc[2])) return false;
                interfaces |= 1u << desc[2];
                ++interface_count;
            }
            midi = desc[5] == 1 && desc[6] == 3 && desc[3] == 0 && desc[7] == 0;
            expected = desc[4]; endpoints = directions = 0; header = false;
            if (midi && (!expected || expected > 2)) return false;
        } else if (type == 5) {
            if (!interface || size < 7 || !(desc[2] & 15) || (desc[2] & 15) >= 8 || (desc[2] & 0x70)) return false;
            if (default_alternate) {
                unsigned bit = (desc[2] & 15) + ((desc[2] >> 7) * 8);
                if (endpoint_addresses & (1u << bit)) return false;
                endpoint_addresses |= 1u << bit;
            }
            if (midi) {
                unsigned direction = desc[2] >> 7;
                if ((desc[3] & 3) != 2 || !word(desc + 4) || word(desc + 4) > 512 ||
                    (directions & (1u << direction))) return false;
                directions |= 1u << direction;
                ++endpoints;
                size_t next = offset + size;
                if (length - next < 4) return false;
                const uint8_t *association = bytes + next;
                if (association[0] < 4 || association[0] > length - next || association[1] != 0x25 ||
                    association[2] != 1 || !association[3] || association[3] > 16 ||
                    association[0] < 4 + association[3]) return false;
            }
        } else if (type == 0x24) {
            if (size < 3) return false;
            if (midi && desc[2] == 1) {
                if (size < 7 || word(desc + 3) != 0x100 || word(desc + 5) < 7 || word(desc + 5) > remaining) return false;
                header = true;
            } else if (midi && desc[2] == 2 && size < 6) return false;
            else if (midi && desc[2] == 3 && (size < 9 || size < 7 + 2u * desc[5])) return false;
        } else if (type == 0x25 && size < 4) return false;
        offset += size;
    }
    return interface_count == bytes[4] && (!midi || (header && endpoints == expected));
}
