/* SPDX-License-Identifier: GPL-2.0-or-later */
#include <command.h>
#include <cpu_func.h>
#include <env.h>
#include <fastboot.h>
#include <hang.h>
#include <image.h>
#include <mmc.h>
#include <net.h>
#include <asm/global_data.h>
#include <asm/io.h>
#include <elf.h>
#include <u-boot/sha256.h>
#include "shoop_boot_state.h"

DECLARE_GLOBAL_DATA_PTR;
static struct blk_desc *card;
static int storage_valid;
static struct shoop_config configuration;
static uint8_t sector_buffer[512] __aligned(ARCH_DMA_MINALIGN);

static int read_sector(void *context, uint32_t lba, void *output) {
    (void)context;
    if (!card || lba >= SHOOP_CARD_SECTORS || blk_dread(card, lba, 1, sector_buffer) != 1) return SHOOP_IO;
    memcpy(output, sector_buffer, 512);
    return 0;
}
static int write_sector(void *context, uint32_t lba, const void *input) {
    (void)context;
    if (lba != SHOOP_STATE_0_LBA && lba != SHOOP_STATE_1_LBA &&
        lba != SHOOP_COUNTER_0_LBA && lba != SHOOP_COUNTER_1_LBA) return SHOOP_INVALID;
    memcpy(sector_buffer, input, 512);
    return blk_dwrite(card, lba, 1, sector_buffer) == 1 ? 0 : SHOOP_IO;
}
static const struct shoop_io io = { NULL, read_sector, write_sector };
static void watchdog(unsigned seconds) {
    writel(0xabc000, 0xf8005000);
    if (seconds) {
        unsigned count = (uint64_t)seconds * 111111111 / (512 * 4096) + 1;
        writel(0x920000 | (count << 2) | 2, 0xf8005004);
        writel(0xabc003, 0xf8005000);
        writel(0x1999, 0xf8005008);
    }
}
static void configure_network(void) {
    char ip[32];
    if (shoop_config_load(&io, &configuration)) {
        memset(&configuration, 0, sizeof configuration);
        env_set("ethaddr", "02:00:00:00:00:01");
        env_set("ipaddr", "10.0.2.15"); env_set("netmask", "255.255.255.0");
        env_set("serverip", "10.0.2.2");
        puts("SHOOP_CONFIGURATION_INVALID recovery network defaults\n");
    } else {
        eth_env_set_enetaddr("ethaddr", configuration.mac);
        ip_to_string((struct in_addr){ .s_addr = configuration.ip }, ip); env_set("ipaddr", ip);
        ip_to_string((struct in_addr){ .s_addr = configuration.netmask }, ip); env_set("netmask", ip);
        ip_to_string((struct in_addr){ .s_addr = configuration.gateway }, ip); env_set("gatewayip", ip);
        ip_to_string((struct in_addr){ .s_addr = configuration.server }, ip); env_set("serverip", ip);
    }
    env_set("netretry", "no");
    env_set("autoload", "no");
    env_set("tftptimeout", "1000"); env_set("tftptimeoutcountmax", "2");
    net_ip = string_to_ip(env_get("ipaddr"));
    net_netmask = string_to_ip(env_get("netmask"));
    net_gateway = string_to_ip(env_get("gatewayip"));
    net_server_ip = string_to_ip(env_get("serverip"));
    printf("SHOOP_NETWORK ip=%s server=%s\n", env_get("ipaddr"), env_get("serverip"));
}
static int open_card(void) {
    struct mmc *mmc = find_mmc_device(0);
    if (!mmc || mmc_init(mmc)) return SHOOP_IO;
    card = mmc_get_blk_desc(mmc);
    if (card->blksz != 512 || card->lba != SHOOP_CARD_SECTORS) return SHOOP_INVALID;
    uint8_t mbr[512];
    if (read_sector(NULL, 0, mbr) || mbr[510] != 0x55 || mbr[511] != 0xaa) return SHOOP_INVALID;
    const uint32_t starts[] = { SHOOP_BOOT_LBA, SHOOP_METADATA_LBA, SHOOP_SLOT_A_LBA, SHOOP_SLOT_B_LBA };
    const uint32_t lengths[] = { SHOOP_BOOT_SECTORS, SHOOP_METADATA_SECTORS, SHOOP_SLOT_A_SECTORS, SHOOP_SLOT_B_SECTORS };
    for (unsigned i = 0; i < 4; ++i) {
        uint32_t start, length;
        memcpy(&start, mbr + 446 + i * 16 + 8, 4); memcpy(&length, mbr + 446 + i * 16 + 12, 4);
        if (start != starts[i] || length != lengths[i]) return SHOOP_INVALID;
    }
    return 0;
}
struct image {
    const uint8_t *elf;
    uint32_t elf_size;
    char build[64];
    uint8_t hash[32];
};
static int validate_elf(const uint8_t *bytes, uint32_t size) {
    Elf32_Ehdr header;
    if (size < sizeof header) return SHOOP_INVALID;
    memcpy(&header, bytes, sizeof header);
    if (memcmp(header.e_ident, "\177ELF\1\1\1", 7) || header.e_type != ET_EXEC || header.e_machine != EM_ARM ||
        header.e_phentsize != sizeof(Elf32_Phdr) || !header.e_phnum || header.e_phnum > 16 ||
        header.e_phoff > size || (uint32_t)header.e_phnum * sizeof(Elf32_Phdr) > size - header.e_phoff) return SHOOP_INVALID;
    uint32_t begin[16], end[16];
    unsigned regions = 0, executable_entry = 0;
    for (unsigned i = 0; i < header.e_phnum; ++i) {
        Elf32_Phdr p;
        memcpy(&p, bytes + header.e_phoff + i * sizeof p, sizeof p);
        if (p.p_type != PT_LOAD) continue;
        if (!p.p_memsz || p.p_vaddr != p.p_paddr || p.p_filesz > p.p_memsz || p.p_offset > size ||
            p.p_filesz > size - p.p_offset || p.p_memsz > UINT32_MAX - p.p_paddr) return SHOOP_INVALID;
        uint32_t limit = p.p_paddr + p.p_memsz;
        if (!((p.p_paddr >= 0x100000 && limit <= 0x08000000) ||
              (p.p_paddr >= 0x08000000 && limit <= SHOOP_STAGE_ADDRESS))) return SHOOP_INVALID;
        for (unsigned n = 0; n < regions; ++n)
            if (p.p_paddr < end[n] && limit > begin[n]) return SHOOP_INVALID;
        begin[regions] = p.p_paddr; end[regions++] = limit;
        if ((p.p_flags & PF_X) && !(header.e_entry & 3) && header.e_entry >= p.p_paddr &&
            header.e_entry - p.p_paddr < p.p_filesz) executable_entry = 1;
    }
    return executable_entry ? 0 : SHOOP_INVALID;
}
static int verify_bundle(const void *fit, uint32_t size, struct image *out) {
    int length;
    if (size < sizeof(struct fdt_header) || size > SHOOP_BUNDLE_MAX || fdt_check_full(fit, size) ||
        fdt_totalsize(fit) != size) return SHOOP_INVALID;
    int config = fdt_path_offset(fit, "/configurations/conf");
    int image = fdt_path_offset(fit, "/images/firmware");
    int key = fdt_path_offset(gd_fdt_blob(), "/signature/key-deployment");
    if (config < 0 || image < 0 || key < 0) return SHOOP_INVALID;
    const char *required = fdt_getprop(gd_fdt_blob(), key, "required", &length);
    if (!required || length != 5 || memcmp(required, "conf", 5) || fit_config_verify(fit, config)) return SHOOP_INVALID;
    const char *name = fdt_getprop(fit, config, "firmware", &length);
    if (!name || length != 9 || memcmp(name, "firmware", 9)) return SHOOP_INVALID;
    const char *profile = fdt_getprop(fit, config, "shoop,profile", &length);
    if (!profile || length != sizeof(SHOOP_PROFILE) || memcmp(profile, SHOOP_PROFILE, sizeof(SHOOP_PROFILE))) return SHOOP_INVALID;
    const char *build = fdt_getprop(fit, config, "shoop,build", &length);
    if (!build || length < 2 || length > 41 || build[length - 1] || strnlen(build, length) != length - 1) return SHOOP_INVALID;
    memset(out, 0, sizeof *out); memcpy(out->build, build, length);
    const uint8_t *data = fdt_getprop(fit, image, "data", &length);
    if (!data || length <= 0 || (uintptr_t)data < (uintptr_t)fit ||
        (uintptr_t)data - (uintptr_t)fit > size || (uint32_t)length > size - ((uintptr_t)data - (uintptr_t)fit) ||
        !fit_image_verify_with_data(fit, image, gd_fdt_blob(), data, length) || validate_elf(data, length)) return SHOOP_INVALID;
    out->elf = data; out->elf_size = length;
    sha256_csum_wd(fit, size, out->hash, CHUNKSZ_SHA256);
    return 0;
}
static int load_slot(uint32_t slot, const uint8_t *expected, struct image *image) {
    struct shoop_slot descriptor;
    uint32_t lba = shoop_slot_lba(slot);
    if (!lba || read_sector(NULL, lba, &descriptor) || !shoop_record_valid(&descriptor, 4) ||
        !descriptor.bundle_bytes || descriptor.bundle_bytes > SHOOP_BUNDLE_MAX ||
        memcmp(descriptor.bundle_hash, expected, 32)) return SHOOP_INVALID;
    uint32_t blocks = (descriptor.bundle_bytes + 511) / 512;
    if (blk_dread(card, lba + 1, blocks, (void *)SHOOP_STAGE_ADDRESS) != blocks ||
        verify_bundle((void *)SHOOP_STAGE_ADDRESS, descriptor.bundle_bytes, image) ||
        memcmp(image->hash, descriptor.bundle_hash, 32) || memcmp(image->build, descriptor.build, 64)) return SHOOP_INVALID;
    return 0;
}
static void update_status(void) {
    struct shoop_state state;
    char value[48];
    if (shoop_state_load(&io, &state)) {
        env_set("fastboot.shoop-state", "recovery");
        env_set("fastboot.shoop-confirmed", "none");
        env_set("fastboot.shoop-pending", "none");
        return;
    }
    env_set("fastboot.shoop-state", state.pending < 2 ? "trial" : "confirmed");
    snprintf(value, sizeof value, "%u", state.confirmed); env_set("fastboot.shoop-confirmed", value);
    snprintf(value, sizeof value, "%u", state.pending); env_set("fastboot.shoop-pending", value);
    snprintf(value, sizeof value, "%llu", state.operation_sequence); env_set("fastboot.shoop-operation", value);
    for (unsigned i = 0; i < 16; ++i) snprintf(value + i * 2, 3, "%02x", state.operation_client[i]);
    env_set("fastboot.shoop-client", value);
    env_set("fastboot.shoop-profile", SHOOP_PROFILE);
}
static int install_bundle(void *data, uint32_t size) {
    struct image image;
    struct shoop_state state;
    if (!storage_valid || (uintptr_t)data != SHOOP_STAGE_ADDRESS || verify_bundle(data, size, &image) ||
        shoop_config_load(&io, &configuration)) return SHOOP_INVALID;
    if (shoop_state_load(&io, &state)) {
        memset(&state, 0, sizeof state); state.generation = 1;
        state.confirmed = state.pending = state.fallback = SHOOP_NO_SLOT;
        puts("SHOOP_RECOVERY_INSTALL reconstructing selection only\n");
    }
    if (state.pending < 2) {
        struct image check;
        return !memcmp(state.pending_hash, image.hash, 32) && !load_slot(state.pending, image.hash, &check)
            ? 0 : SHOOP_CONFLICT;
    }
    if (state.confirmed < 2 && !memcmp(state.confirmed_hash, image.hash, 32)) {
        void *saved = (void *)(SHOOP_STAGE_ADDRESS + SHOOP_BUNDLE_MAX);
        struct image check;
        memcpy(saved, data, size);
        int valid = !load_slot(state.confirmed, image.hash, &check);
        memcpy(data, saved, size);
        if (valid) {
            state.mode = SHOOP_BOOT_NORMAL;
            return shoop_state_store(&io, &state);
        }
    }
    uint32_t slot = state.confirmed < 2 ? state.confirmed ^ 1u : 0;
    if (state.fallback == slot) {
        state.fallback = SHOOP_NO_SLOT; memset(state.fallback_hash, 0, 32);
    }
    if (shoop_state_store(&io, &state)) return SHOOP_IO;
    uint32_t lba = shoop_slot_lba(slot), blocks = (size + 511) / 512;
    memset((uint8_t *)data + size, 0, blocks * 512 - size);
    printf("SHOOP_INSTALL_BEGIN slot=%u bytes=%u\n", slot, size);
    if (blk_dwrite(card, lba + 1, blocks, data) != blocks ||
        blk_dread(card, lba + 1, blocks, data) != blocks) return SHOOP_IO;
    uint8_t readback_hash[32];
    sha256_csum_wd(data, size, readback_hash, CHUNKSZ_SHA256);
    if (memcmp(readback_hash, image.hash, 32)) return SHOOP_IO;
    struct shoop_slot descriptor = { .bundle_bytes = size };
    memcpy(descriptor.bundle_hash, image.hash, 32); memcpy(descriptor.build, image.build, 64);
    shoop_record_seal(&descriptor, 4); memcpy(sector_buffer, &descriptor, 512);
    if (blk_dwrite(card, lba, 1, sector_buffer) != 1) return SHOOP_IO;
    struct image check;
    if (load_slot(slot, image.hash, &check)) return SHOOP_IO;
    puts("SHOOP_INSTALL_VERIFIED\n");
    int result = shoop_candidate(&io, &state, slot, image.hash);
    if (!result) printf("SHOOP_INSTALL_PENDING slot=%u build=%s\n", slot, image.build);
    return result;
}
void fastboot_oem_board(char *parameter, void *data, u32 size, char *response) {
    if (!strcmp(parameter, "install")) {
        int result = install_bundle(data, size);
        update_status();
        if (result) fastboot_fail(result == SHOOP_CONFLICT ? "pending candidate conflict" : "image or storage validation failed", response);
        else fastboot_okay("candidate verified", response);
    } else if (!strcmp(parameter, "status")) {
        update_status(); fastboot_okay("query shoop getvars", response);
    } else {
        fastboot_fail("unsupported board operation", response);
    }
}
static int launch(const struct image *image, uint32_t slot, const struct shoop_state *state) {
    if (gd->relocaddr < 0x3c000000 || gd->start_addr_sp < 0x3c000000) return SHOOP_INVALID;
    struct shoop_handoff handoff = { .magic = SHOOP_HANDOFF_MAGIC, .version = SHOOP_VERSION,
        .slot = slot, .trial = slot < 2 && slot == state->pending, .config = configuration };
    if (shoop_nonce_reserve(&io, &handoff.nonce)) return SHOOP_IO;
    memcpy(handoff.bundle_hash, image->hash, 32); memcpy(handoff.build, image->build, 64);
    memcpy(handoff.operation_client, state->operation_client, 16); handoff.operation_sequence = state->operation_sequence;
    handoff.crc = shoop_crc32(&handoff, 508);
    printf("SHOOP_BOOT_IMAGE slot=%u trial=%u nonce=%llu build=%s\n", slot, handoff.trial, handoff.nonce, image->build);
    eth_halt();
    unsigned long entry = load_elf_image_phdr((unsigned long)image->elf);
    memcpy((void *)SHOOP_HANDOFF_ADDRESS, &handoff, sizeof handoff);
    flush_cache(SHOOP_HANDOFF_ADDRESS, sizeof handoff);
    watchdog(30);
    cleanup_before_linux();
    ((void (*)(void))entry)();
    hang();
}
static int network_boot(struct shoop_state *state) {
    if (run_command("tftpboot 0x38000000 shoop.itb", 0)) return SHOOP_IO;
    ulong size = env_get_hex("filesize", 0);
    struct image image;
    if (size > SHOOP_BUNDLE_MAX || verify_bundle((void *)SHOOP_STAGE_ADDRESS, size, &image)) return SHOOP_INVALID;
    return launch(&image, SHOOP_NO_SLOT, state);
}
static int boot_local(struct shoop_state *state) {
    uint32_t slot;
    struct image image;
    if (shoop_select(&io, state, &slot)) return SHOOP_INVALID;
    const uint8_t *expected = slot == state->pending ? state->pending_hash : state->confirmed_hash;
    if (load_slot(slot, expected, &image)) {
        printf("SHOOP_IMAGE_REFUSED slot=%u\n", slot);
        if (slot != state->pending) return SHOOP_INVALID;
        state->pending = SHOOP_NO_SLOT; state->attempts = 0;
        memset(state->pending_hash, 0, 32);
        if (shoop_state_store(&io, state) || state->confirmed >= 2 ||
            load_slot(state->confirmed, state->confirmed_hash, &image)) return SHOOP_INVALID;
        slot = state->confirmed;
    }
    return launch(&image, slot, state);
}
static int do_shoop(struct cmd_tbl *command, int flags, int argc, char *const argv[]) {
    (void)command; (void)flags;
    watchdog(0);
    if (argc != 2 || (strcmp(argv[1], "boot") && strcmp(argv[1], "netboot") && strcmp(argv[1], "recovery"))) return CMD_RET_USAGE;
    int storage = open_card();
    storage_valid = !storage;
    configure_network();
    if (storage) puts("SHOOP_CARD_INVALID\n");
    else {
        struct shoop_state state;
        if (!shoop_state_load(&io, &state) && strcmp(argv[1], "recovery")) {
            uint32_t mode = state.mode;
            if (mode != SHOOP_BOOT_NORMAL) {
                state.mode = SHOOP_BOOT_NORMAL;
                if (shoop_state_store(&io, &state)) mode = SHOOP_BOOT_UPDATE;
            }
            if (mode == SHOOP_BOOT_NETWORK || !strcmp(argv[1], "netboot")) {
                int result = network_boot(&state);
                printf("SHOOP_NETWORK_BOOT_REFUSED result=%d\n", result);
            }
            if (mode != SHOOP_BOOT_UPDATE && !boot_local(&state)) return 0;
        }
    }
    for (;;) {
        update_status();
        puts("SHOOP_UPDATER_READY\n");
        run_command("fastboot udp", 0);
        struct shoop_state state;
        if (!storage && !shoop_state_load(&io, &state)) boot_local(&state);
    }
}
U_BOOT_CMD(shoop, 2, 0, do_shoop, "Shoop SD boot and network deployment", "boot | netboot | recovery");
