#ifndef SHOOP_BSPCONFIG_H
#define SHOOP_BSPCONFIG_H
#define EL1_NONSECURE 0
/* Leave USE_AMP undefined: xil_cache.c tests #ifndef USE_AMP and skips all L2
   maintenance when it is defined, even as 0, which breaks GEM DMA coherency. */
#define SIM_MODE 0
#define FREERTOS 1
#define PLATFORM_ZYNQ 1
#endif
