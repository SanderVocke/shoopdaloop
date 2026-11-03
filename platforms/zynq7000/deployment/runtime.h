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
};
_Static_assert(sizeof(struct shoop_deployment_info) == 144, "deployment info ABI");
#ifdef SHOOP_DEPLOYMENT
#include "shoop_boot_state.h"
int shoop_deploy_init(void);
const struct shoop_config *shoop_deploy_config(void);
void shoop_deploy_tick(void);
void shoop_deploy_render(void);
void shoop_deploy_broker(void);
void shoop_deploy_control(void);
void shoop_deploy_render_stopped(void);
void shoop_deploy_stopped(void);
int shoop_deploy_confirm(void);
int shoop_deploy_info(struct shoop_deployment_info *);
int shoop_deploy_request(const uint8_t *, uint64_t);
void shoop_deploy_reset(void);
#else
static inline void shoop_deploy_tick(void) {}
static inline void shoop_deploy_render(void) {}
static inline void shoop_deploy_broker(void) {}
static inline void shoop_deploy_control(void) {}
static inline void shoop_deploy_render_stopped(void) {}
static inline void shoop_deploy_stopped(void) {}
#endif
#endif
