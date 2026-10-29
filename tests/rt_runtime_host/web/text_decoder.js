// AudioWorklet lacks TextDecoder on some Chromium versions. Only binding error
// messages use this fallback; successful rendering exchanges numeric values.
export const decoderFallback = typeof globalThis.TextDecoder === "undefined";
if (decoderFallback) {
    globalThis.TextDecoder = class {
        decode(input = new Uint8Array()) {
            let result = "";
            for (let i = 0; i < input.length;) {
                const first = input[i++];
                let value, remaining;
                if (first < 0x80) { value = first; remaining = 0; }
                else if (first >= 0xc2 && first < 0xe0) { value = first & 31; remaining = 1; }
                else if (first < 0xf0 && first >= 0xe0) { value = first & 15; remaining = 2; }
                else if (first >= 0xf0 && first < 0xf5) { value = first & 7; remaining = 3; }
                else { throw new Error("invalid UTF-8 binding message"); }
                for (let n = 0; n < remaining; n++) {
                    const next = input[i++];
                    if (next === undefined || (next & 0xc0) !== 0x80) throw new Error("truncated UTF-8 binding message");
                    value = (value << 6) | (next & 63);
                }
                if (value < [0, 0x80, 0x800, 0x10000][remaining] || value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)) throw new Error("invalid UTF-8 scalar");
                result += String.fromCodePoint(value);
            }
            return result;
        }
    };
}
