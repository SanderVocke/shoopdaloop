#include "tusb.h"
#include "host/hcd.h"
#include "host/hub.h"
#include "descriptors.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

#define NODES 8
struct node {
    bool hub, attached, unsupported, composite, multiple_midi;
    uint8_t protocol, rx, tx, ports;
    tusb_speed_t speed;
    struct node *child[31];
    uint16_t changes[31];
    uint32_t enabled;
};
struct transfer {uint8_t *buffer; uint16_t length; bool active;};
static struct node nodes[NODES], *addresses[7], *enumerating, *root;
static struct transfer pending[7][16][2];
static tusb_control_request_t request;
static uint32_t now, mounts, unmounts, errors, setups, hub_refusals;
static uint8_t midi_index, midi_address, midi_endpoint;
static bool hang_control;
static unsigned malformed;
static uint8_t configuration[256];

uint32_t tusb_time_millis_api(void) {return now;}
void tusb_time_delay_ms_api(uint32_t ms) {now += ms;}
bool hcd_init(uint8_t p, const tusb_rhport_init_t *i) {(void)p; (void)i; return true;}
bool hcd_deinit(uint8_t p) {(void)p; return true;}
void hcd_int_enable(uint8_t p) {(void)p;}
void hcd_int_disable(uint8_t p) {(void)p;}
void hcd_int_handler(uint8_t p, bool isr) {(void)p; (void)isr;}
uint32_t hcd_frame_number(uint8_t p) {(void)p; return now;}
bool hcd_port_connect_status(uint8_t p) {(void)p; return root && root->attached;}
void hcd_port_reset(uint8_t p) {(void)p; enumerating = root;}
void hcd_port_reset_end(uint8_t p) {(void)p;}
tusb_speed_t hcd_port_speed_get(uint8_t p) {(void)p; return root->speed;}
void hcd_device_close(uint8_t p, uint8_t a) {(void)p; memset(pending[a], 0, sizeof(pending[a])); addresses[a] = NULL;}
bool hcd_edpt_open(uint8_t p, uint8_t a, const tusb_desc_endpoint_t *d) {(void)p; (void)a; (void)d; return true;}
bool hcd_edpt_close(uint8_t p, uint8_t a, uint8_t e) {(void)p; pending[a][e & 15][e >> 7].active = false; return true;}
bool hcd_edpt_abort_xfer(uint8_t p, uint8_t a, uint8_t e) {return hcd_edpt_close(p,a,e);}
bool hcd_edpt_clear_stall(uint8_t p, uint8_t a, uint8_t e) {(void)p; (void)a; (void)e; return true;}

static unsigned config(struct node *node) {
    unsigned n;
    if (node->hub) {
        const uint8_t bytes[] = {9,2,25,0,1,1,0,0x80,0, 9,4,0,0,1,9,0,node->protocol == 2 ? 1 : node->protocol,0,
                                7,5,0x81,3,1,0,12};
        memcpy(configuration, bytes, sizeof(bytes)); n = sizeof(bytes);
        if (node->protocol == 2) {
            const uint8_t alternate[] = {9,4,0,1,1,9,0,2,0,7,5,0x81,3,1,0,12};
            memcpy(configuration+n, alternate, sizeof(alternate)); n += sizeof(alternate);
            configuration[2] = n;
        }
    } else {
        const uint8_t bytes[] = {9,2,0,0,1,1,0,0x80,0, 9,4,0,0,0,1,3,0,0,
                                7,0x24,1,0,1,0,0};
        memcpy(configuration, bytes, sizeof(bytes)); n = sizeof(bytes);
        if (node->composite) {
            const uint8_t extra[] = {9,4,0,0,0,1,1,0,0,9,0x24,1,0,1,9,0,1,1,
                                     9,4,1,0,0,1,2,0,0};
            memmove(configuration+9+sizeof(extra),configuration+9,n-9);
            memcpy(configuration+9,extra,sizeof(extra)); n += sizeof(extra);
            configuration[4] = 3; configuration[9+sizeof(extra)+2] = 2;
        }
        unsigned endpoints = (node->rx != 0) + (node->tx != 0);
        unsigned midi_offset = node->composite ? 36 : 9;
        configuration[midi_offset+4] = endpoints;
        if (node->unsupported) configuration[midi_offset+5] = 0xff;
        for (unsigned direction = 0; direction < 2; ++direction) {
            unsigned cables = direction ? node->tx : node->rx;
            if (!cables) continue;
            uint8_t endpoint[] = {9,5,direction ? 1 : 0x81,2,64,0,0,0,0};
            memcpy(configuration+n, endpoint, sizeof(endpoint)); n += sizeof(endpoint);
            configuration[n++] = 4+cables;
            configuration[n++] = 0x25; configuration[n++] = 1; configuration[n++] = cables;
            for (unsigned i=0;i<cables;++i) configuration[n++] = i+1;
        }
        unsigned header_length = n - midi_offset - 9;
        configuration[midi_offset+14] = header_length;
        configuration[midi_offset+15] = header_length >> 8;
        if (node->multiple_midi) {
            unsigned count = n-midi_offset, second = n;
            assert(n+count <= sizeof(configuration));
            memcpy(configuration+second, configuration+midi_offset, count);
            ++configuration[4]; ++configuration[second+2];
            for (unsigned offset=second; offset<second+count; offset+=configuration[offset])
                if (configuration[offset+1]==5) ++configuration[offset+2];
            n += count;
        }
        configuration[2] = n; configuration[3] = n>>8;
        if (malformed == 1) configuration[9] = 0;
        if (malformed == 2) configuration[2] = configuration[3] = 0xff;
        if (malformed == 5) configuration[midi_offset+7] = 2;
    }
    return n;
}

bool hcd_setup_send(uint8_t p, uint8_t a, const uint8_t bytes[8]) {
    (void)p;
    assert(a < 7);
    memcpy(&request, bytes, 8); ++setups;
    struct node *node = a ? addresses[a] : enumerating;
    assert(node);
    if (request.bmRequestType_bit.type == TUSB_REQ_TYPE_STANDARD && request.bRequest == TUSB_REQ_SET_ADDRESS)
        addresses[request.wValue] = enumerating;
    if (node->hub && request.bmRequestType_bit.type == TUSB_REQ_TYPE_CLASS && request.wIndex) {
        unsigned port = request.wIndex - 1; assert(port < 31);
        if (request.bRequest == HUB_REQUEST_SET_FEATURE && request.wValue == HUB_FEATURE_PORT_RESET) {
            enumerating = node->child[port]; node->enabled |= 1u << port; node->changes[port] |= 16;
        }
        if (request.bRequest == HUB_REQUEST_CLEAR_FEATURE && request.wValue >= 16)
            node->changes[port] &= ~(1u << (request.wValue - 16));
    }
    if (!hang_control) hcd_event_xfer_complete(a, 0, 8, XFER_RESULT_SUCCESS, false);
    return true;
}

bool hcd_edpt_xfer(uint8_t p, uint8_t a, uint8_t ep, uint8_t *buffer, uint16_t length) {
    (void)p;
    assert(a < 7);
    if (ep & 15) {
        assert(!pending[a][ep & 15][ep >> 7].active);
        pending[a][ep & 15][ep >> 7] = (struct transfer){buffer,length,true};
        return true;
    }
    struct node *node = a ? addresses[a] : enumerating;
    unsigned actual = length;
    uint8_t response[256] = {0};
    if ((ep & 0x80) && length) {
        if (request.bmRequestType_bit.type == TUSB_REQ_TYPE_STANDARD && request.bRequest == TUSB_REQ_GET_DESCRIPTOR) {
            switch (request.wValue >> 8) {
            case TUSB_DESC_DEVICE: {
                const uint8_t device[] = {18,1,0,2,node->hub ? 9 : 0,0,node->hub ? node->protocol : 0,
                                         64,0xfe,0xca,0x10,0x40,0,1,0,0,0,1};
                actual = malformed == 4 && length > 8 ? 8 : sizeof(device); memcpy(response,device,actual); break;
            }
            case TUSB_DESC_CONFIGURATION:
                actual = config(node);
                if (malformed == 3 && length > 9) actual = 8;
                memcpy(response,configuration,actual); break;
            default: actual = 0; break;
            }
        } else if (node->hub && request.bRequest == HUB_REQUEST_GET_DESCRIPTOR) {
            const uint8_t hub[] = {9,0x29,node->ports ? node->ports : 4,1,0,1,0,0,0xff};
            actual = sizeof(hub); memcpy(response,hub,actual);
        } else if (node->hub && request.bRequest == HUB_REQUEST_GET_STATUS && request.wIndex) {
            unsigned port = request.wIndex-1;
            struct node *child = node->child[port];
            uint16_t status = 0x100;
            if (child && child->attached) status |= 1 | ((node->enabled & (1u << port)) ? 2 : 0) |
                (child->speed == TUSB_SPEED_HIGH ? 0x400 : child->speed == TUSB_SPEED_LOW ? 0x200 : 0);
            response[0] = status; response[1] = status >> 8;
            response[2] = node->changes[port]; response[3] = node->changes[port] >> 8; actual = 4;
        }
        if (actual > length) actual = length;
        memcpy(buffer,response,actual);
    }
    hcd_event_xfer_complete(a,ep,actual,XFER_RESULT_SUCCESS,false);
    return true;
}

void tuh_hub_descriptor_failed_cb(uint8_t address) {(void)address; ++hub_refusals;}

void tuh_midi_mount_cb(uint8_t index, const tuh_midi_mount_cb_t *info) {
    ++mounts; midi_index=index; midi_address=info->daddr;
    struct node *node=addresses[info->daddr];
    midi_endpoint = 1 + info->bInterfaceNumber - (node->composite ? 2 : 0);
    assert(info->rx_cable_count == node->rx && info->tx_cable_count == node->tx);
}
void tuh_midi_umount_cb(uint8_t index) {(void)index; ++unmounts;}
void tuh_midi_error_cb(uint8_t index, uint8_t endpoint, xfer_result_t result) {
    (void)index; (void)endpoint; assert(result != XFER_RESULT_SUCCESS); ++errors;
}

bool tuh_enum_descriptor_configuration_cb(uint8_t address, uint8_t index, const tusb_desc_configuration_t *descriptor) {
    (void)address; (void)index;
    return shoop_usb_configuration_valid((const uint8_t *)descriptor, descriptor->wTotalLength);
}

static void progress(unsigned count) {
    while (count--) {
        ++now; tuh_task_ext(0,false);
        for (unsigned a=1;a<7;++a) {
            struct node *node=addresses[a];
            struct transfer *transfer=&pending[a][1][1];
            if (!node || !node->hub || !transfer->active) continue;
            unsigned bitmap=0;
            for (unsigned p=0;p<31;++p) if (node->changes[p]) bitmap |= 2u << p;
            if (bitmap) {
                unsigned count=transfer->length; assert(count<=4);
                for (unsigned i=0;i<count;++i) transfer->buffer[i]=bitmap>>(i*8);
                transfer->active=false;
                hcd_event_xfer_complete(a,0x81,count,XFER_RESULT_SUCCESS,false);
            }
        }
    }
}
static void begin(struct node *node) {
    root=node; root->attached=true; hcd_event_device_attach(0,false); progress(3000);
}
static void remove_root(void) {
    root->attached=false; hcd_event_device_remove(0,false); progress(100);
}
static void packet_roundtrip(void) {
    struct transfer *rx=&pending[midi_address][midi_endpoint][1];
    assert(rx->active && rx->length>=8);
    const uint8_t packets[]={9,0x90,60,100,8,0x80,60,0};
    memcpy(rx->buffer,packets,8); rx->active=false;
    hcd_event_xfer_complete(midi_address,0x80 | midi_endpoint,8,XFER_RESULT_SUCCESS,false); progress(10);
    uint8_t packet[4];
    for (unsigned i=0;i<2;++i) {
        assert(tuh_midi_packet_read(midi_index,packet)); assert(!memcmp(packet,packets+4*i,4));
        assert(tuh_midi_packet_write(midi_index,packet));
    }
    tuh_midi_write_flush(midi_index);
    struct transfer *tx=&pending[midi_address][midi_endpoint][0]; assert(tx->active);
    assert(tx->length>=4 && !memcmp(tx->buffer,packets,4));
    tx->active=false; hcd_event_xfer_complete(midi_address,midi_endpoint,tx->length,XFER_RESULT_SUCCESS,false); progress(10);
    rx=&pending[midi_address][midi_endpoint][1]; assert(rx->active); rx->active=false;
    hcd_event_xfer_complete(midi_address,0x80 | midi_endpoint,0,XFER_RESULT_STALLED,false); progress(10);
    assert(errors);
}

int main(void) {
    tusb_rhport_init_t init={.role=TUSB_ROLE_HOST,.speed=TUSB_SPEED_HIGH}; assert(tusb_init(0,&init));
    nodes[0]=(struct node){.speed=TUSB_SPEED_HIGH,.rx=16,.tx=16};
    begin(&nodes[0]); assert(mounts==1); packet_roundtrip(); remove_root(); assert(unmounts==1);
    for (unsigned i=0;i<100;++i) {begin(&nodes[0]); remove_root();}
    assert(mounts==101 && unmounts==101);
    nodes[0].rx=1; nodes[0].tx=0; begin(&nodes[0]); assert(mounts==102); remove_root();
    nodes[0].rx=0; nodes[0].tx=1; begin(&nodes[0]); assert(mounts==103); remove_root();
    nodes[0].rx=nodes[0].tx=1;
    malformed=true; begin(&nodes[0]); assert(mounts==103); remove_root(); malformed=false;
    for (malformed=2;malformed<=5;++malformed) {
        begin(&nodes[0]); assert(mounts==103); remove_root();
    }
    malformed=0;
    hang_control=true; begin(&nodes[0]); progress(5000); assert(mounts==103); remove_root(); hang_control=false;
    for (unsigned protocol=0;protocol<=2;++protocol) {
        nodes[1]=(struct node){.hub=true,.protocol=protocol,.speed=protocol ? TUSB_SPEED_HIGH : TUSB_SPEED_FULL};
        nodes[2]=(struct node){.hub=true,.protocol=protocol,.speed=protocol ? TUSB_SPEED_HIGH : TUSB_SPEED_FULL,.attached=true};
        nodes[0].attached=true; nodes[0].speed=TUSB_SPEED_FULL;
        nodes[1].child[0]=&nodes[2]; nodes[1].changes[0]=1;
        nodes[2].child[1]=&nodes[0]; nodes[2].changes[1]=1;
        begin(&nodes[1]); progress(5000);
        assert(mounts==104+protocol);
        tuh_bus_info_t info; assert(tuh_bus_info_get(midi_address,&info));
        assert(info.hub_addr && info.hub_port==2 && info.speed==TUSB_SPEED_FULL);
        remove_root(); assert(unmounts==104+protocol);
    }
    uint32_t before = mounts, removed = unmounts;
    nodes[1]=(struct node){.hub=true,.protocol=1,.speed=TUSB_SPEED_HIGH};
    for (unsigned i=0;i<4;++i) {
        unsigned index=i ? i+1 : 0;
        nodes[index]=(struct node){.speed=i%2 ? TUSB_SPEED_HIGH : TUSB_SPEED_FULL,
            .rx=2,.tx=2,.attached=true,.composite=i==2,.multiple_midi=true};
        nodes[1].child[i]=&nodes[index]; nodes[1].changes[i]=1;
    }
    begin(&nodes[1]); progress(10000); assert(mounts==before+8);
    nodes[0].attached=false; nodes[1].changes[0]=1; progress(3000);
    assert(unmounts==removed+2); packet_roundtrip();
    nodes[0].attached=true; nodes[1].changes[0]=1; progress(3000);
    assert(mounts==before+10); remove_root(); assert(unmounts==removed+10);
    before=mounts;
    nodes[4].unsupported=true;
    for (unsigned i=0;i<4;++i) {nodes[1].child[i]->attached=true; nodes[1].changes[i]=1;}
    begin(&nodes[1]); progress(10000); assert(mounts==before+6); remove_root();
    before=mounts;
    nodes[1]=(struct node){.hub=true,.protocol=1,.speed=TUSB_SPEED_HIGH};
    nodes[2]=(struct node){.hub=true,.protocol=1,.speed=TUSB_SPEED_HIGH,.attached=true};
    for (unsigned i=0;i<5;++i) {
        unsigned index=i ? i+2 : 0;
        nodes[index]=(struct node){.speed=TUSB_SPEED_FULL,.rx=1,.tx=1,.attached=true};
        struct node *hub=i<3 ? &nodes[1] : &nodes[2];
        unsigned port=i<3 ? i : i-3;
        hub->child[port]=&nodes[index]; hub->changes[port]=1;
    }
    nodes[1].child[3]=&nodes[2]; nodes[1].changes[3]=1;
    begin(&nodes[1]); progress(15000); assert(mounts==before+4); remove_root();
    for (unsigned port=8;port<=31;port+=port==8 ? 7 : 16) {
        before=mounts;
        nodes[1]=(struct node){.hub=true,.protocol=1,.speed=TUSB_SPEED_HIGH,.ports=31};
        nodes[0]=(struct node){.speed=TUSB_SPEED_FULL,.rx=1,.tx=1,.attached=true};
        nodes[1].child[port-1]=&nodes[0]; nodes[1].changes[port-1]=1;
        begin(&nodes[1]); progress(10000); assert(mounts==before+1); packet_roundtrip();
        nodes[0].attached=false; nodes[1].changes[port-1]=1; progress(3000);
        assert(unmounts==mounts); remove_root();
    }
    nodes[1]=(struct node){.hub=true,.protocol=1,.speed=TUSB_SPEED_HIGH,.ports=32};
    begin(&nodes[1]); assert(hub_refusals==1); remove_root();
    assert(tuh_deinit(0));
    printf("TinyUSB fake HCD: %u mounts, %u unmounts, %u control requests; directions, stalls, timeout, malformed config and FS/STT/MTT nested hubs passed\n",mounts,unmounts,setups);
    return 0;
}
