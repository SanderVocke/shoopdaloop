// SPDX-License-Identifier: GPL-3.0-or-later
#define main unused_abi_benchmark_main
#include "abi_benchmark.cpp"
#undef main
#include <xmmintrin.h>
int main(int argc,char**argv) try {
    require(argc==5 || argc==6,"usage: denormal_probe native-library resources synthetic-plugin output.csv [voyage-state.xml]");
    const bool voyage=argc==6;frames=voyage?1024:128;
    void *library=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);require(library,"runtime unavailable");
    auto get=reinterpret_cast<const NativePluginDescriptor*(*)()>(dlsym(library,"carla_get_native_rack_plugin"));require(get,"descriptor absent");
    const auto *d=get();NativeHostDescriptor host{};host.resourceDir=argv[2];host.uiName="denormal diagnostic";
    host.get_buffer_size=buffer_size;host.get_sample_rate=sample_rate;host.is_offline=offline;host.get_time_info=get_time;host.write_midi_event=midi;host.dispatcher=dispatch;
    std::string xml;
    if(voyage) { std::ifstream f(argv[5]);xml.assign(std::istreambuf_iterator<char>(f),{}); }
    else xml="<?xml version='1.0'?><!DOCTYPE CARLA-PROJECT><CARLA-PROJECT VERSION='2.5'><Plugin><Info><Type>LADSPA</Type><Name>Subnormal diagnostic</Name><Label>shoop_denormal</Label><Binary>"+std::string(argv[3])+"</Binary><UniqueID>590002</UniqueID></Info><Data><Active>Yes</Active><Options>0x0</Options></Data></Plugin></CARLA-PROJECT>";
    std::vector<float> input(frames),left(frames),right(frames),prime(frames);
    std::fill(input.begin(),input.end(),voyage?0.0f:1e-38f);
    for(unsigned i=0;i<frames;++i)prime[i]=0.125f*std::sin(i*.173f);
    float *in[]={input.data(),input.data()},*out[]={left.data(),right.data()},*warm[]={prime.data(),prime.data()};
    const unsigned saved=_mm_getcsr();
    struct Row {unsigned repeat,ftz,block,actual;uint64_t wall,cpu;};std::vector<Row> rows;rows.reserve(40000);
    for(unsigned repeat=0;repeat<(std::getenv("SHOOP_BENCH_SMOKE")?1u:10u);++repeat)for(unsigned order=0;order<2;++order) {
        unsigned enabled=repeat%2?1-order:order;
        auto handle=d->instantiate(&host);require(handle,"instantiate failed");d->set_state(handle,xml.c_str());d->activate(handle);
        _mm_setcsr(enabled?saved|0x8040:saved&~0x8040u);
        for(unsigned block=0;block<200;++block)d->process(handle,voyage?warm:in,out,frames,nullptr,0);
        if(!voyage)require(enabled?left[0]==0:std::fpclassify(left[0])==FP_SUBNORMAL,"unexpected floating-point mode/output");
        for(unsigned block=0;block<(std::getenv("SHOOP_BENCH_SMOKE")?10u:2000u);++block) {
            auto wall=ns(CLOCK_MONOTONIC),cpu=ns(CLOCK_THREAD_CPUTIME_ID);
            d->process(handle,in,out,frames,nullptr,0);
            cpu=ns(CLOCK_THREAD_CPUTIME_ID)-cpu;wall=ns(CLOCK_MONOTONIC)-wall;
            if(voyage) { for(float v:left)require(std::isfinite(v) && std::fabs(v)<8,"invalid Voyage tail"); }
            else require(enabled?left[0]==0:std::fpclassify(left[0])==FP_SUBNORMAL,"output changed mode");
            rows.push_back({repeat,enabled,block,_mm_getcsr(),wall,cpu});
        }
        d->deactivate(handle);d->cleanup(handle);
    }
    _mm_setcsr(saved);
    std::ofstream f(argv[4]);f<<"repeat,ftz_daz,block,mxcsr_after,wall_ns,cpu_ns\n";for(const auto&r:rows)f<<r.repeat<<','<<r.ftz<<','<<r.block<<','<<r.actual<<','<<r.wall<<','<<r.cpu<<'\n';
    std::cout<<"validated "<<(voyage?"Voyage silent-tail":"subnormal")<<" diagnostic; original MXCSR="<<saved<<"; same Carla/plugin binaries throughout\n";
    return 0;
} catch(const std::exception&e){std::cerr<<e.what()<<'\n';return 1;}
