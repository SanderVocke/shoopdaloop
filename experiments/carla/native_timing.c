/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <CarlaNative.h>
#include <lv2/lv2.h>
#include <lv2/urid.h>
#include <lv2/options.h>
#include <lv2/buf-size.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#if defined(__SSE__)
#include <xmmintrin.h>
#endif

typedef struct { uint64_t begin, end, cpu, frames; } Sample;
typedef struct { uint64_t count, capacity, sample_rate, buffer_size; Sample samples[16384]; uint64_t fp_before, fp_after; } Capture;
static Capture *capture;
static void *library;
static void (*original_process)(NativePluginHandle,float**,float**,uint32_t,const NativeMidiEvent*,uint32_t);
static uint64_t now(clockid_t c) { struct timespec t; clock_gettime(c,&t); return (uint64_t)t.tv_sec*1000000000+t.tv_nsec; }
static uint64_t fp_mode(void) {
#if defined(__SSE__)
    return _mm_getcsr();
#else
    return 0;
#endif
}
static void process(NativePluginHandle h,float **in,float **out,uint32_t n,const NativeMidiEvent *m,uint32_t nm) {
    if(capture)capture->fp_before=fp_mode();
    uint64_t begin=now(CLOCK_MONOTONIC), cpu=now(CLOCK_THREAD_CPUTIME_ID);
    original_process(h,in,out,n,m,nm);
    cpu=now(CLOCK_THREAD_CPUTIME_ID)-cpu;
    uint64_t end=now(CLOCK_MONOTONIC);
    if(capture)capture->fp_after=fp_mode();
    if(capture && capture->count<capture->capacity) {
        capture->samples[capture->count++]=(Sample){begin,end,cpu,n};
    }
}
static const NativePluginDescriptor *descriptor(const char *symbol) {
    if(!library) {
        const char *path=getenv("SHOOP_BENCH_REAL_CARLA");
        if(!path || !(library=dlopen(path,RTLD_NOW|RTLD_LOCAL))) { fprintf(stderr,"real Carla unavailable\n"); abort(); }
        const char *output=getenv("SHOOP_BENCH_CAPTURE");
        if(output) {
            int fd=open(output,O_RDWR|O_CREAT|O_TRUNC,0600);
            if(fd<0 || ftruncate(fd,sizeof(Capture))) abort();
            capture=mmap(NULL,sizeof(Capture),PROT_READ|PROT_WRITE,MAP_SHARED,fd,0); close(fd);
            if(capture==MAP_FAILED) abort();
            capture->capacity=16384;
        }
    }
    const NativePluginDescriptor *(*get)(void)=dlsym(library,symbol);
    if(!get) abort();
    const NativePluginDescriptor *base=get();
    NativePluginDescriptor *copy=malloc(sizeof(*copy)); memcpy(copy,base,sizeof(*copy));
    if(original_process && original_process!=base->process) abort();
    original_process=base->process; copy->process=process;
    return copy;
}
const NativePluginDescriptor *carla_get_native_rack_plugin(void) { return descriptor("carla_get_native_rack_plugin"); }
const NativePluginDescriptor *carla_get_native_patchbay_plugin(void) { return descriptor("carla_get_native_patchbay_plugin"); }
const NativePluginDescriptor *carla_get_native_patchbay16_plugin(void) { return descriptor("carla_get_native_patchbay16_plugin"); }

static LV2_Handle (*original_instantiate_lv2)(const LV2_Descriptor*,double,const char*,const LV2_Feature*const*);
static LV2_Handle instantiate_lv2(const LV2_Descriptor* d,double rate,const char* bundle,const LV2_Feature*const* features) {
    const LV2_URID_Map *map=NULL; const LV2_Options_Option *options=NULL;
    for(unsigned i=0;features[i];++i) {
        if(!strcmp(features[i]->URI,LV2_URID__map)) map=features[i]->data;
        if(!strcmp(features[i]->URI,LV2_OPTIONS__options)) options=features[i]->data;
    }
    if(capture) {
        capture->sample_rate=(uint64_t)rate;
        if(map && options) for(unsigned i=0;options[i].key;++i)
            if(options[i].key==map->map(map->handle,LV2_BUF_SIZE__nominalBlockLength)) capture->buffer_size=*(const uint32_t*)options[i].value;
    }
    return original_instantiate_lv2(d,rate,bundle,features);
}
static void (*original_run)(LV2_Handle,uint32_t);
static void run_lv2(LV2_Handle h,uint32_t n) {
    if(capture)capture->fp_before=fp_mode();
    uint64_t begin=now(CLOCK_MONOTONIC), cpu=now(CLOCK_THREAD_CPUTIME_ID);
    original_run(h,n);
    cpu=now(CLOCK_THREAD_CPUTIME_ID)-cpu; uint64_t end=now(CLOCK_MONOTONIC);
    if(capture)capture->fp_after=fp_mode();
    if(capture && capture->count<capture->capacity) capture->samples[capture->count++]=(Sample){begin,end,cpu,n};
}
const LV2_Descriptor *lv2_descriptor(uint32_t i) {
    if(!library) {
        const char *path=getenv("SHOOP_BENCH_REAL_CARLA");
        if(!path || !(library=dlopen(path,RTLD_NOW|RTLD_LOCAL))) abort();
        const char *output=getenv("SHOOP_BENCH_CAPTURE");
        if(output) {
            int fd=open(output,O_RDWR|O_CREAT|O_TRUNC,0600);
            if(fd<0 || ftruncate(fd,sizeof(Capture))) abort();
            capture=mmap(NULL,sizeof(Capture),PROT_READ|PROT_WRITE,MAP_SHARED,fd,0); close(fd);
            if(capture==MAP_FAILED) abort(); capture->capacity=16384;
        }
    }
    const LV2_Descriptor *(*get)(uint32_t)=dlsym(library,"lv2_descriptor");
    if(!get) abort(); const LV2_Descriptor *base=get(i); if(!base) return NULL;
    LV2_Descriptor *copy=malloc(sizeof(*copy)); memcpy(copy,base,sizeof(*copy));
    if(original_run && original_run!=base->run) abort();
    original_instantiate_lv2=base->instantiate; copy->instantiate=instantiate_lv2; original_run=base->run; copy->run=run_lv2; return copy;
}
const void *lv2ui_descriptor(uint32_t i) {
    if(!library) (void)lv2_descriptor(0);
    const void *(*get)(uint32_t)=dlsym(library,"lv2ui_descriptor");
    return get ? get(i) : NULL;
}
