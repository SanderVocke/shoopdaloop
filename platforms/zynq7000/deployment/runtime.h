/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef SHOOP_DEPLOY_RUNTIME_H
#define SHOOP_DEPLOY_RUNTIME_H
#include <stdint.h>
struct shoop_deployment_info {
    uint32_t enabled, slot, confirmed, trial;
    uint64_t nonce;
    char build[64];
    uint8_t bundle_hash[32], operation_client[16];
    uint64_t operation_sequence;
    char profile[32];
};
_Static_assert(sizeof(struct shoop_deployment_info) == 176, "deployment info ABI");
#ifdef SHOOP_DEPLOYMENT
#include "shoop_boot_state.h"
int shoop_deploy_init(void);
const struct shoop_config *shoop_deploy_config(void);
void shoop_deploy_tick(void);
void shoop_deploy_render(void);
void shoop_deploy_broker(void);
void shoop_deploy_control(void);
void shoop_deploy_render_stopped(void);
void shoop_deploy_render_started(void);
void shoop_deploy_stopped(void);
int shoop_deploy_confirm(void);
int shoop_deploy_info(struct shoop_deployment_info *);
int shoop_deploy_request(const uint8_t *, uint64_t);
void shoop_deploy_reset(void);
#else
#include "watchdog.h"
static inline void shoop_deploy_tick(void) { shoop_watchdog_tick(); }
static inline void shoop_deploy_render(void) { shoop_watchdog_render(); }
static inline void shoop_deploy_broker(void) { shoop_watchdog_broker(); }
static inline void shoop_deploy_control(void) { shoop_watchdog_control(); }
static inline void shoop_deploy_render_stopped(void) { shoop_watchdog_render_stopped(); }
static inline void shoop_deploy_render_started(void) { shoop_watchdog_render_started(); }
static inline void shoop_deploy_stopped(void) { shoop_watchdog_stopped(); }
#endif
#endif
