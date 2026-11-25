#include "crash.h"

static uint32_t checksum(const volatile struct shoop_crash_record *record) {
    uint32_t crc = UINT32_MAX;
    for (unsigned word = 1; word < SHOOP_CRASH_WORDS; ++word) {
        if (word == SHOOP_CRASH_CHECKSUM) continue;
        uint32_t value = record->word[word];
        for (unsigned byte = 0; byte < 4; ++byte) {
            crc ^= value & 255;
            value >>= 8;
            for (unsigned bit = 0; bit < 8; ++bit)
                crc = (crc >> 1) ^ (0xedb88320u & (0u - (crc & 1)));
        }
    }
    return ~crc;
}

void shoop_crash_store(volatile struct shoop_crash_record *record,
                      const struct shoop_crash_info *info, const struct shoop_crash_io *io) {
    record->word[SHOOP_CRASH_SEAL] = 0;
    record->word[SHOOP_CRASH_SCHEMA] = SHOOP_CRASH_VERSION;
    record->word[SHOOP_CRASH_LENGTH] = SHOOP_CRASH_WORDS;
    record->word[SHOOP_CRASH_KIND] = info->kind;
    record->word[SHOOP_CRASH_PC] = info->pc;
    record->word[SHOOP_CRASH_CPSR] = info->cpu.cpsr;
    record->word[SHOOP_CRASH_SPSR] = info->cpu.spsr;
    record->word[SHOOP_CRASH_DFSR] = info->cpu.dfsr;
    record->word[SHOOP_CRASH_DFAR] = info->cpu.dfar;
    record->word[SHOOP_CRASH_IFSR] = info->cpu.ifsr;
    record->word[SHOOP_CRASH_IFAR] = info->cpu.ifar;
    record->word[SHOOP_CRASH_TASK] = info->task;
    record->word[SHOOP_CRASH_BUILD] = info->build;
    record->word[14] = 0;
    record->word[15] = 0;
    record->word[SHOOP_CRASH_CHECKSUM] = checksum(record);
    io->flush(io->context, record, sizeof(*record));
    record->word[SHOOP_CRASH_SEAL] = SHOOP_CRASH_MAGIC;
    io->flush(io->context, record, sizeof(*record));
}

int shoop_crash_valid(const volatile struct shoop_crash_record *record) {
    return record->word[SHOOP_CRASH_SEAL] == SHOOP_CRASH_MAGIC &&
        record->word[SHOOP_CRASH_SCHEMA] == SHOOP_CRASH_VERSION &&
        record->word[SHOOP_CRASH_LENGTH] == SHOOP_CRASH_WORDS &&
        record->word[SHOOP_CRASH_CHECKSUM] == checksum(record);
}

static int emit(const struct shoop_crash_io *io, uint32_t *polls, unsigned char byte) {
    while (*polls) {
        --*polls;
        if (io->ready(io->context)) {
            io->write(io->context, byte);
            return 1;
        }
    }
    return 0;
}

size_t shoop_crash_emit(const volatile struct shoop_crash_record *record,
                       const struct shoop_crash_io *io, uint32_t polls) {
    static const char prefix[] = "SHOOP_ARM_EXCEPTION";
    static const char hex[] = "0123456789abcdef";
    size_t sent = 0;
    if (!shoop_crash_valid(record)) return 0;
    for (unsigned i = 0; i < sizeof(prefix)-1; ++i) {
        if (!emit(io, &polls, prefix[i])) return sent;
        ++sent;
    }
    for (unsigned i = 0; i < SHOOP_CRASH_WORDS; ++i) {
        if (!emit(io, &polls, ' ')) return sent;
        ++sent;
        uint32_t value = record->word[i];
        for (unsigned digit = 0; digit < 8; ++digit) {
            if (!emit(io, &polls, hex[(value >> 28) & 15])) return sent;
            ++sent;
            value <<= 4;
        }
    }
    if (emit(io, &polls, '\r')) ++sent;
    if (emit(io, &polls, '\n')) ++sent;
    return sent;
}
