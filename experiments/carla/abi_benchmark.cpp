// SPDX-License-Identifier: GPL-3.0-or-later
#include <CarlaNative.h>
#include <lv2/lv2.h>
#include <lv2/urid.h>
#include <lv2/options.h>
#include <lv2/buf-size.h>
#include <lv2/atom.h>
#include <lv2/state.h>
#include <dlfcn.h>
#include <time.h>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <map>
#include <stdexcept>
#include <string>
#include <vector>

static uint64_t ns(clockid_t id) { timespec t{}; clock_gettime(id,&t); return uint64_t(t.tv_sec)*1000000000+t.tv_nsec; }
struct EngineInfo { float load; uint32_t xruns; };
static uint32_t frames;
static NativeTimeInfo time_info{};
static uint32_t buffer_size(void*) { return frames; }
static double sample_rate(void*) { return 48000; }
static bool offline(void*) { return false; }
static const NativeTimeInfo* get_time(void*) { return &time_info; }
static bool midi(void*,const NativeMidiEvent*) { return true; }
static intptr_t dispatch(void*,NativeHostDispatcherOpcode,int32_t,intptr_t,void*,float) { return 0; }
static std::map<std::string,uint32_t> urids;
static uint32_t map_uri(void*,const char* s) { auto it=urids.find(s); if(it!=urids.end()) return it->second; auto n=urids.size()+1; urids[s]=n; return n; }
static std::string state;
static LV2_State_Status store_chunk(LV2_State_Handle handle,uint32_t key,const void* value,size_t size,uint32_t type,uint32_t) {
    if(key==map_uri(nullptr,"http://kxstudio.sf.net/ns/carla/chunk") && type==map_uri(nullptr,LV2_ATOM__String)) static_cast<std::string*>(handle)->assign(static_cast<const char*>(value),size-1);
    return LV2_STATE_SUCCESS;
}
static const void* retrieve(void*,uint32_t key,size_t* size,uint32_t* type,uint32_t* flags) {
    if(key!=map_uri(nullptr,"http://kxstudio.sf.net/ns/carla/chunk")) return nullptr;
    *size=state.size()+1; *type=map_uri(nullptr,LV2_ATOM__String); *flags=LV2_STATE_IS_POD|LV2_STATE_IS_PORTABLE; return state.c_str();
}
static void require(bool b,const char* s) { if(!b) throw std::runtime_error(s); }
int main(int argc,char** argv) try {
    require(argc==8,"usage: abi_benchmark native|lv2 library bundle state.xml frames blocks output.csv");
    std::string mode=argv[1]; frames=std::stoul(argv[5]); auto count=std::stoul(argv[6]);
    require(frames>0 && frames<=8192 && count>0,"invalid frames/blocks");
    std::ifstream file(argv[4]); require(bool(file),"missing state"); state.assign(std::istreambuf_iterator<char>(file),{});
    void* library=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL); if(!library) throw std::runtime_error(dlerror());
    std::vector<float> left(frames,0.125f),right(frames,0.25f),out_l(frames),out_r(frames);
    const bool voyage=state.find("<Type>VST3</Type>")!=std::string::npos && state.find("Voyage")!=std::string::npos;
    if(voyage) {
        const char* path=std::getenv("SHOOP_BENCH_VOYAGE_INPUT"); require(path,"missing Voyage input path");
        std::ifstream input(path,std::ios::binary); std::vector<float> data(16384); input.read(reinterpret_cast<char*>(data.data()),65536); require(input.gcount()==65536,"invalid Voyage input");
        std::copy_n(data.data(),frames,left.data()); std::copy_n(data.data()+8192,frames,right.data());
    }
    float* inputs[]={left.data(),right.data()}; float* outputs[]={out_l.data(),out_r.data()};
    const NativePluginDescriptor* nd=nullptr; NativePluginHandle nh=nullptr;
    void* meter_handle=nullptr;
    const EngineInfo* (*meter_info)(void*)=nullptr;
    void (*meter_free)(void*)=nullptr;
    const LV2_Descriptor* ld=nullptr; LV2_Handle lh=nullptr;
    NativeHostDescriptor host{}; host.resourceDir=argv[3]; host.uiName="benchmark";
    host.get_buffer_size=buffer_size; host.get_sample_rate=sample_rate; host.is_offline=offline;
    host.get_time_info=get_time; host.write_midi_event=midi; host.dispatcher=dispatch;
    LV2_URID_Map mapper{nullptr,map_uri}; int32_t block=frames;
    LV2_Options_Option options[]={{LV2_OPTIONS_INSTANCE,0,map_uri(nullptr,LV2_BUF_SIZE__nominalBlockLength),sizeof(block),map_uri(nullptr,LV2_ATOM__Int),&block},{}};
    LV2_Feature fm{LV2_URID__map,&mapper},fo{LV2_OPTIONS__options,options}; const LV2_Feature* features[]={&fm,&fo,nullptr};
    std::vector<uint64_t> in_atom(8192),out_atom(8192); float freewheel=0;
    auto* in_seq=reinterpret_cast<LV2_Atom_Sequence*>(in_atom.data()); auto* out_seq=reinterpret_cast<LV2_Atom_Sequence*>(out_atom.data());
    if(mode=="native") {
        auto get=reinterpret_cast<const NativePluginDescriptor*(*)()>(dlsym(library,"carla_get_native_rack_plugin")); require(get,"missing native descriptor");
        nd=get(); nh=nd->instantiate(&host); require(nh,"native instantiate failed"); nd->set_state(nh,state.c_str()); nd->activate(nh);
        auto create=reinterpret_cast<void*(*)(const NativePluginDescriptor*,NativePluginHandle)>(dlsym(library,"carla_create_native_plugin_host_handle"));
        if(create) {
            std::string path=argv[2]; path=path.substr(0,path.find_last_of('/'))+"/libcarla_host-plugin.so";
            void* api=dlopen(path.c_str(),RTLD_NOW|RTLD_LOCAL);
            if(api) { meter_info=reinterpret_cast<const EngineInfo*(*)(void*)>(dlsym(api,"carla_get_runtime_engine_info")); meter_free=reinterpret_cast<void(*)(void*)>(dlsym(library,"carla_host_handle_free")); meter_handle=create(nd,nh); }
        }
    } else {
        require(mode=="lv2","unknown mode");
        auto get=reinterpret_cast<const LV2_Descriptor*(*)(uint32_t)>(dlsym(library,"lv2_descriptor")); require(get,"missing LV2 descriptor");
        for(uint32_t i=0; (ld=get(i));++i) if(std::string(ld->URI)=="http://kxstudio.sf.net/carla/plugins/carlarack") break;
        require(ld,"rack LV2 URI absent"); lh=ld->instantiate(ld,48000,argv[3],features); require(lh,"LV2 instantiate failed");
        ld->connect_port(lh,0,in_seq); ld->connect_port(lh,1,out_seq); ld->connect_port(lh,2,&freewheel);
        ld->connect_port(lh,3,inputs[0]); ld->connect_port(lh,4,inputs[1]); ld->connect_port(lh,5,outputs[0]); ld->connect_port(lh,6,outputs[1]);
        auto si=static_cast<const LV2_State_Interface*>(ld->extension_data(LV2_STATE__interface)); require(si,"missing state interface");
        require(si->restore(lh,retrieve,nullptr,0,features)==0,"LV2 restore failed"); ld->activate(lh);
    }
    auto process=[&]() {
        if(nd) nd->process(nh,inputs,outputs,frames,nullptr,0);
        else { in_seq->atom.type=map_uri(nullptr,LV2_ATOM__Sequence); in_seq->atom.size=sizeof(LV2_Atom_Sequence_Body); out_seq->atom.size=out_atom.size()*8-sizeof(LV2_Atom); ld->run(lh,frames); }
    };
    const bool synthetic=state.find("shoop_benchmark")!=std::string::npos;
    const bool gain=state.find("audiogain_s")!=std::string::npos;
    auto expected=[&](float x) { if(synthetic) { for(unsigned i=0;i<192;++i) x=x*0.9999f+0.00001f; return x*0.5f; } return gain?x*0.5f:x; };
    auto validate=[&]() {
        if(voyage) {
            for(unsigned channel=0;channel<2;++channel) {
                double energy=0;bool altered=false;
                for(size_t i=0;i<frames;++i) { require(std::isfinite(outputs[channel][i]),"nonfinite Voyage output"); const double sample=outputs[channel][i]; energy+=sample*sample; altered=altered||std::fabs(outputs[channel][i]-inputs[channel][i])>1e-5f; }
                require(energy/frames>1e-10 && energy/frames<16 && altered,"Voyage output was silent, bypassed or unbounded");
            }
            return;
        }
        for(size_t i=0;i<frames;++i) require(std::isfinite(out_l[i]) && std::fabs(out_l[i]-expected(left[i]))<2e-5f && std::fabs(out_r[i]-expected(right[i]))<2e-5f,"output validation failed"); };
    for(unsigned i=0;i<200;++i) process(); validate();
    std::vector<uint64_t> wall(count),cpu(count); std::vector<float> dsp(count);
    for(size_t i=0;i<count;++i) { auto w=ns(CLOCK_MONOTONIC); auto c=ns(CLOCK_THREAD_CPUTIME_ID); process(); cpu[i]=ns(CLOCK_THREAD_CPUTIME_ID)-c; wall[i]=ns(CLOCK_MONOTONIC)-w; if(meter_info && meter_handle) dsp[i]=meter_info(meter_handle)->load; validate(); }
    if(meter_handle && meter_free) meter_free(meter_handle);
    if(nd) { auto saved=nd->get_state(nh); require(saved,"state unavailable"); if(synthetic) require(std::string(saved).find("shoop_benchmark")!=std::string::npos,"plugin not loaded"); if(voyage) require(std::string(saved).find("<Type>VST3</Type>")!=std::string::npos,"VST3 not loaded"); free(saved); nd->deactivate(nh); nd->cleanup(nh); }
    else {
        if(voyage) { std::string chunk; auto si=static_cast<const LV2_State_Interface*>(ld->extension_data(LV2_STATE__interface)); require(si->save(lh,store_chunk,&chunk,0,features)==0 && chunk.find("<Type>VST3</Type>")!=std::string::npos,"VST3 not loaded through LV2"); }
        ld->deactivate(lh); ld->cleanup(lh);
    }
    std::ofstream result(argv[7]); result<<"block,wall_ns,cpu_ns,dsp_percent\n"; for(size_t i=0;i<count;++i) result<<i<<','<<wall[i]<<','<<cpu[i]<<','<<dsp[i]<<'\n';
    std::cout<<"validated "<<mode<<" frames="<<frames<<" blocks="<<count<<'\n';
    return 0;
} catch(const std::exception& e) { std::cerr<<e.what()<<'\n'; return 1; }
