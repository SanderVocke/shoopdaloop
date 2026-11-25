/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef SHOOP_WATCHDOG_H
#define SHOOP_WATCHDOG_H
#ifdef SHOOP_HOST_WATCHDOG
void shoop_watchdog_start(void);
int shoop_watchdog_ready(void);
void shoop_watchdog_tick(void);
void shoop_watchdog_control(void);
void shoop_watchdog_render(void);
void shoop_watchdog_broker(void);
void shoop_watchdog_render_stopped(void);
void shoop_watchdog_render_started(void);
void shoop_watchdog_stopped(void);
#else
static inline void shoop_watchdog_start(void) {}
static inline int shoop_watchdog_ready(void) { return 1; }
static inline void shoop_watchdog_tick(void) {}
static inline void shoop_watchdog_control(void) {}
static inline void shoop_watchdog_render(void) {}
static inline void shoop_watchdog_broker(void) {}
static inline void shoop_watchdog_render_stopped(void) {}
static inline void shoop_watchdog_render_started(void) {}
static inline void shoop_watchdog_stopped(void) {}
#endif
#endif
