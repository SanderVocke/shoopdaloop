#pragma once
#include <stddef.h>
#include <stdint.h>

#define SHOOP_CRASH_MAGIC 0x53444352u
#define SHOOP_CRASH_VERSION 1u
#define SHOOP_CRASH_WORDS 16u

enum shoop_crash_word {
    SHOOP_CRASH_SEAL, SHOOP_CRASH_SCHEMA, SHOOP_CRASH_LENGTH, SHOOP_CRASH_CHECKSUM,
    SHOOP_CRASH_KIND, SHOOP_CRASH_PC, SHOOP_CRASH_CPSR, SHOOP_CRASH_SPSR,
    SHOOP_CRASH_DFSR, SHOOP_CRASH_DFAR, SHOOP_CRASH_IFSR, SHOOP_CRASH_IFAR,
    SHOOP_CRASH_TASK, SHOOP_CRASH_BUILD
};
struct shoop_crash_registers { uint32_t cpsr, spsr, dfsr, dfar, ifsr, ifar; };
struct shoop_crash_info {
    uint32_t kind, pc;
    struct shoop_crash_registers cpu;
    uint32_t task, build;
};
struct shoop_crash_record { uint32_t word[SHOOP_CRASH_WORDS]; };
struct shoop_crash_io {
    void *context;
    void (*flush)(void *, const volatile void *, size_t);
    int (*ready)(void *);
    void (*write)(void *, unsigned char);
};
_Static_assert(sizeof(struct shoop_crash_record) == 64, "crash record ABI");
_Static_assert(sizeof(struct shoop_crash_registers) == 24, "exception register ABI");

void shoop_crash_store(volatile struct shoop_crash_record *, const struct shoop_crash_info *, const struct shoop_crash_io *);
int shoop_crash_valid(const volatile struct shoop_crash_record *);
size_t shoop_crash_emit(const volatile struct shoop_crash_record *, const struct shoop_crash_io *, uint32_t polls);
void shoop_exceptions_init(void);
void shoop_exception_registers(struct shoop_crash_registers *);
void shoop_exception_mask(void);
void shoop_exception_halt(void) __attribute__((noreturn));
