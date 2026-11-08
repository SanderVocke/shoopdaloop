# Pi 4 audio chunk benchmark bundle

Download `shoop-pi4-chunks-arm64-d40bfdaa.zip` and extract it. The ZIP contains the ARM64 host and
launcher, all three renderer modes, bundled runtime libraries, scripts and a
short README describing the tests and result export.

Requires 64-bit Linux ARM64 and glibc 2.36 or newer (Pi OS Bookworm/Debian 12+).

- Binary source: `d40bfdaa` (includes upstream Pi optimization PR #958).
- Helper source: `5c5009cc0992900867016f8240fbbd935421e66c`.
- SHA-256: `812db902905d467812ee85a8803f214bafd343bfb524b190a76e6342bb2e50f4`.
- Built for Cortex-A72 with AES/SHA extensions disabled for Pi 4 compatibility.
- ARM64 app smoke checks pass under emulation; emulated timings are not Pi measurements.

[Read the test guide](https://github.com/SanderVocke/shoopdaloop/blob/5c5009cc0992900867016f8240fbbd935421e66c/docs/design/chunks/PI_BENCHMARKS.md).
