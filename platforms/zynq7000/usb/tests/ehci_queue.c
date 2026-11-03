#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "ehci.c"

static tuh_bus_info_t topology[7];
static unsigned queries;

bool tuh_bus_info_get(uint8_t address, tuh_bus_info_t *info) {
    assert(address < 7 && ++queries <= 4);
    *info = topology[address];
    return true;
}

void usbh_spin_lock(bool isr) {(void)isr; assert(false);}
void usbh_spin_unlock(bool isr) {(void)isr; assert(false);}

static void check(uint8_t address, uint8_t hub, uint8_t port, tusb_speed_t speed) {
    const tusb_desc_endpoint_t endpoint = {.bLength=7,.bDescriptorType=5,
        .bEndpointAddress=0x81,.bmAttributes={.xfer=TUSB_XFER_BULK},.wMaxPacketSize=64};
    ehci_qhd_t queue;
    memset(&queue,0,sizeof(queue)); queries=0;
    qhd_init(&queue,address,&endpoint);
    assert(queue.fl_hub_addr == hub && queue.fl_hub_port == port && queue.ep_speed == speed);
}

int main(void) {
    topology[1]=(tuh_bus_info_t){.speed=TUSB_SPEED_FULL};
    check(1,0,0,TUSB_SPEED_FULL);
    topology[5]=(tuh_bus_info_t){.speed=TUSB_SPEED_HIGH};
    topology[1]=(tuh_bus_info_t){.speed=TUSB_SPEED_FULL,.hub_addr=5,.hub_port=3};
    check(1,5,3,TUSB_SPEED_FULL);
    topology[6]=(tuh_bus_info_t){.speed=TUSB_SPEED_FULL,.hub_addr=5,.hub_port=7};
    topology[1]=(tuh_bus_info_t){.speed=TUSB_SPEED_FULL,.hub_addr=6,.hub_port=2};
    check(1,5,7,TUSB_SPEED_FULL);
    topology[5].speed=TUSB_SPEED_FULL;
    check(1,0,0,TUSB_SPEED_FULL);
    topology[5].speed=TUSB_SPEED_HIGH;
    topology[1].speed=TUSB_SPEED_LOW;
    check(1,5,7,TUSB_SPEED_LOW);
    topology[1]=(tuh_bus_info_t){.speed=TUSB_SPEED_HIGH,.hub_addr=5,.hub_port=4};
    check(1,5,4,TUSB_SPEED_HIGH);
    tusb_desc_endpoint_t invalid = {.bLength=7,.bDescriptorType=5,.bEndpointAddress=0x81,
        .bmAttributes={.xfer=TUSB_XFER_INTERRUPT},.wMaxPacketSize=1};
    queries=0; assert(!hcd_edpt_open(0,1,&invalid));
    invalid.bInterval=17; queries=0; assert(!hcd_edpt_open(0,1,&invalid));
    topology[1].speed=TUSB_SPEED_FULL; invalid.bInterval=0;
    queries=0; assert(!hcd_edpt_open(0,1,&invalid));
    puts("Actual EHCI queue heads select the nearest high-speed TT, nested FS and root integrated TT routes; invalid interrupt intervals refused before schedule access");
    return 0;
}
