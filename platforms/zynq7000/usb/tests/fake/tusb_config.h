#ifndef SHOOP_FAKE_TUSB_CONFIG_H
#define SHOOP_FAKE_TUSB_CONFIG_H
#include "../../tusb_config.h"
#undef CFG_TUSB_OS
#define CFG_TUSB_OS OPT_OS_NONE
#undef CFG_TUH_MEM_SECTION
#define CFG_TUH_MEM_SECTION
#endif
