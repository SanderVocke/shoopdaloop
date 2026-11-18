#pragma once

int shoop_native_audio_available(void);
int shoop_native_audio_start(void);
int shoop_native_audio_wait(void *owner, int stopping);

#ifdef SHOOP_FIXTURE
#include <stdint.h>

void shoop_fixture_native_audio_tick(void);
uint32_t shoop_fixture_native_audio_read(uint32_t offset);
void shoop_fixture_native_audio_write(uint32_t offset, uint32_t value);
#endif
