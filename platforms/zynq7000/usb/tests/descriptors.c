#include "descriptors.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    uint8_t configuration[] = {
        9,2,53,0,1,1,0,0x80,0, 9,4,0,0,2,1,3,0,0, 7,0x24,1,0,1,35,0,
        9,5,0x81,2,64,0,0,0,0, 5,0x25,1,1,1,
        9,5,1,2,64,0,0,0,0, 5,0x25,1,1,1,
    };
    assert(sizeof(configuration)==53);
    assert(shoop_usb_configuration_valid(configuration,sizeof(configuration)));
    for (unsigned length=0;length<sizeof(configuration);++length)
        assert(!shoop_usb_configuration_valid(configuration,length));
    const unsigned fields[]={0,1,2,4,5,9,13,18,22,23,25,27,28,29,34,35,36,37,39,41,48,49,50,51};
    for (unsigned i=0;i<sizeof(fields)/sizeof(fields[0]);++i) {
        uint8_t broken[sizeof(configuration)]; memcpy(broken,configuration,sizeof(broken));
        broken[fields[i]]=0;
        assert(!shoop_usb_configuration_valid(broken,sizeof(broken)));
    }
    uint8_t malformed[4096];
    for (unsigned position=9;position<sizeof(configuration);++position) {
        for (unsigned value=0;value<256;++value) {
            memcpy(malformed,configuration,sizeof(configuration)); malformed[position]=value;
            shoop_usb_configuration_valid(malformed,sizeof(configuration));
        }
    }
    puts("USB configuration bounds, truncated descriptors, endpoint associations and malformed fields passed");
}
