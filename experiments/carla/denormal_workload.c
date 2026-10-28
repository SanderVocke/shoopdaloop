/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <ladspa/ladspa.h>
#include <stdlib.h>
typedef struct { float *input, *output; } Instance;
static LADSPA_Handle instantiate(const LADSPA_Descriptor *d,unsigned long rate) {
    (void)d;(void)rate;return calloc(1,sizeof(Instance));
}
static void connect_port(LADSPA_Handle h,unsigned long port,float *value) {
    if(port==0)((Instance*)h)->input=value;else ((Instance*)h)->output=value;
}
static void run(LADSPA_Handle h,unsigned long frames) {
    Instance *v=h;
    for(unsigned long i=0;i<frames;++i) {
        float x=v->input[i];
        for(unsigned j=0;j<192;++j)x*=0.9999f;
        v->output[i]=x;
    }
}
static const LADSPA_PortDescriptor ports[]={LADSPA_PORT_INPUT|LADSPA_PORT_AUDIO,LADSPA_PORT_OUTPUT|LADSPA_PORT_AUDIO};
static const char *names[]={"Input","Output"};
static const LADSPA_PortRangeHint hints[]={{0,0,0},{0,0,0}};
static const LADSPA_Descriptor descriptor={
    .UniqueID=590002,.Label="shoop_denormal",.Properties=LADSPA_PROPERTY_HARD_RT_CAPABLE,
    .Name="Shoop subnormal diagnostic",.Maker="ShoopDaLoop investigation",.Copyright="GPL-3.0-or-later",
    .PortCount=2,.PortDescriptors=ports,.PortNames=names,.PortRangeHints=hints,
    .instantiate=instantiate,.connect_port=connect_port,.run=run,.cleanup=free
};
const LADSPA_Descriptor *ladspa_descriptor(unsigned long i){return i==0?&descriptor:NULL;}
