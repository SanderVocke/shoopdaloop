/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <ladspa/ladspa.h>
#include <stdlib.h>

typedef struct { float *ports[3]; } Instance;
static LADSPA_Handle instantiate(const LADSPA_Descriptor *d, unsigned long rate) {
    (void)d; (void)rate; return calloc(1, sizeof(Instance));
}
static void connect_port(LADSPA_Handle h, unsigned long p, float *v) {
    ((Instance *)h)->ports[p] = v;
}
static void run(LADSPA_Handle h, unsigned long n) {
    Instance *v = h;
    for (unsigned long i = 0; i < n; ++i) {
        float x = v->ports[0][i];
        /* A stable recurrence, with bounded work that affects the output. */
        for (unsigned j = 0; j < 192; ++j) x = x * 0.9999f + 0.00001f;
        v->ports[1][i] = x * 0.5f;
    }
    *v->ports[2] = (float)n;
}
static const LADSPA_PortDescriptor ports[] = {
    LADSPA_PORT_INPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_OUTPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_OUTPUT | LADSPA_PORT_CONTROL
};
static const char *names[] = {"Input", "Output", "Processed frames"};
static const LADSPA_PortRangeHint hints[] = {{0,0,0},{0,0,0},{0,0,0}};
static const LADSPA_Descriptor descriptor = {
    .UniqueID=590001, .Label="shoop_benchmark", .Properties=LADSPA_PROPERTY_HARD_RT_CAPABLE,
    .Name="Shoop deterministic DSP workload", .Maker="ShoopDaLoop investigation",
    .Copyright="GPL-3.0-or-later", .PortCount=3, .PortDescriptors=ports,
    .PortNames=names, .PortRangeHints=hints, .instantiate=instantiate,
    .connect_port=connect_port, .run=run, .cleanup=free
};
const LADSPA_Descriptor *ladspa_descriptor(unsigned long i) { return i == 0 ? &descriptor : NULL; }
