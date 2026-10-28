// SPDX-License-Identifier: GPL-3.0-or-later
#include <dlfcn.h>
#include <xmmintrin.h>
#include <cstdio>
int main(int argc,char **argv) {
    if(argc!=2)return 2;
    unsigned before=_mm_getcsr();
    void *library=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);
    if(!library){std::fprintf(stderr,"%s\n",dlerror());return 1;}
    unsigned after=_mm_getcsr();
    std::printf("before=0x%x after=0x%x before_ftz_daz=0x%x after_ftz_daz=0x%x\n",before,after,before&0x8040,after&0x8040);
    return 0;
}
