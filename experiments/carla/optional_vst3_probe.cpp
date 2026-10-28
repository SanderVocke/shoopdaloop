// SPDX-License-Identifier: GPL-3.0-or-later
#define main unused_abi_benchmark_main
#include "abi_benchmark.cpp"
#undef main
int main(int argc,char**argv) try {
    require(argc==4 || argc==5,"usage: optional_vst3_probe native-library resources vst3-bundle");
    frames=1024;void *library=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);require(library,"runtime unavailable");
    auto get=reinterpret_cast<const NativePluginDescriptor*(*)()>(dlsym(library,"carla_get_native_rack_plugin"));require(get,"descriptor absent");
    const auto *d=get();NativeHostDescriptor host{};host.resourceDir=argv[2];host.uiName="optional VST3 probe";
    host.get_buffer_size=buffer_size;host.get_sample_rate=sample_rate;host.is_offline=offline;host.get_time_info=get_time;host.write_midi_event=midi;host.dispatcher=dispatch;
    auto handle=d->instantiate(&host);require(handle,"instantiate failed");
    std::string xml="<?xml version='1.0'?><!DOCTYPE CARLA-PROJECT><CARLA-PROJECT VERSION='2.5'><Plugin><Info><Type>VST3</Type><Name>Voyage Voyage</Name><Binary>"+std::string(argv[3])+"</Binary><Label></Label></Info><Data><Active>Yes</Active><Options>0x0</Options></Data></Plugin></CARLA-PROJECT>";
    d->set_state(handle,xml.c_str());char *saved=d->get_state(handle);require(saved,"state unavailable");
    if(argc==5) { std::ofstream state_file(argv[4]); state_file<<saved; }
    const bool loaded=std::string(saved).find("<Type>VST3</Type>")!=std::string::npos;free(saved);d->cleanup(handle);
    std::cout<<"VST3 plugin loaded="<<loaded<<'\n';return loaded?0:1;
} catch(const std::exception&e){std::cerr<<e.what()<<'\n';return 1;}
