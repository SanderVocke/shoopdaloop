#include "FreeRTOS.h"
#include "task.h"
#include "lwip/init.h"
#include "lwip/tcpip.h"
#include "lwip/sockets.h"
#include "netif/xadapter.h"
#include "xil_printf.h"
#include "xuartps_hw.h"
#include <string.h>

static struct netif interface;

static void echo(void *unused) {
    (void)unused;
    int listener = lwip_socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address = { .sin_family = AF_INET, .sin_port = PP_HTONS(15400) };
    configASSERT(listener >= 0);
    configASSERT(lwip_bind(listener, (struct sockaddr *)&address, sizeof(address)) == 0);
    configASSERT(lwip_listen(listener, 4) == 0);
    xil_printf("SHOOP_ECHO_READY\r\n");
    for (;;) {
        int peer = lwip_accept(listener, NULL, NULL);
        if (peer < 0) continue;
        char bytes[2048];
        int count;
        while ((count = lwip_recv(peer, bytes, sizeof(bytes), 0)) > 0) {
            for (int offset = 0; offset < count;) {
                int sent = lwip_send(peer, bytes + offset, count - offset, 0);
                if (sent <= 0) break;
                offset += sent;
            }
        }
        lwip_close(peer);
    }
}
static void network(void *unused) {
    (void)unused;
    ip_addr_t ip, mask, gateway;
    IP4_ADDR(&ip, 10, 0, 2, 15); IP4_ADDR(&mask, 255, 255, 255, 0); IP4_ADDR(&gateway, 10, 0, 2, 2);
    unsigned char mac[] = { 0x02, 0, 0, 0, 0, 1 };
    lwip_init();
    configASSERT(xemac_add(&interface, &ip, &mask, &gateway, mac, SHOOP_GEM_BASE));
    netif_set_default(&interface); netif_set_up(&interface);
    sys_thread_new("gem-input", (void (*)(void *))xemacif_input_thread, &interface, 4096, 4);
    sys_thread_new("echo", echo, NULL, 4096, 3);
    vTaskDelete(NULL);
}
int main(void) {
    extern void shoop_platform_init(void);
    shoop_platform_init();
    xil_printf("SHOOP_BOOT FreeRTOS AMD-lwIP board=%s ram_mib=%u\r\n",
               SHOOP_BOARD_PROFILE, (unsigned)(SHOOP_RAM_BYTES / (1024 * 1024)));
    configASSERT(xTaskCreate(network, "network", 8192, NULL, 3, NULL) == pdPASS);
    vTaskStartScheduler();
    vApplicationAssert("scheduler returned", 0);
    return 1;
}
