#include "FreeRTOS.h"
#include "xtime_l.h"
#include "task.h"
#include "xscugic.h"
#include "xscutimer.h"
#include "xemacps.h"
#include "xuartps.h"
#include "xil_printf.h"
#include "xil_assert.h"
#include "xil_io.h"
#include "temperature.h"
#include "netif/xtopology.h"
#include <sys/stat.h>
#include <errno.h>
#include <string.h>
#ifdef SHOOP_BOARD_PHY_YT8531
#include "yt8531.h"
#include "sleep.h"
#endif
static char *c_heap_next;

XScuGic_Config XScuGic_ConfigTable[] = {{ .DeviceId = 0, .CpuBaseAddress = 0xf8f00100, .DistBaseAddress = 0xf8f01000 }};
XScuTimer_Config XScuTimer_ConfigTable[] = {{ .DeviceId = 0, .BaseAddr = 0xf8f00600 }};
XEmacPs_Config XEmacPs_ConfigTable[] = {{ .DeviceId = 0, .BaseAddress = SHOOP_GEM_BASE, .S1GDiv0 = SHOOP_GEM_1000_DIV0, .S1GDiv1 = SHOOP_GEM_1000_DIV1, .S100MDiv0 = SHOOP_GEM_100_DIV0, .S100MDiv1 = SHOOP_GEM_100_DIV1, .S10MDiv0 = SHOOP_GEM_10_DIV0, .S10MDiv1 = SHOOP_GEM_10_DIV1 }};
struct xtopology_t xtopology[] = {{ .emac_baseaddr = SHOOP_GEM_BASE, .emac_type = xemac_type_emacps, .scugic_baseaddr = 0xf8f01000, .scugic_emac_intr = SHOOP_GEM_IRQ }};
int xtopology_n_emacs = 1;

#if defined(SHOOP_FPGA_PROBE) || defined(SHOOP_FPGA) || defined(SHOOP_NATIVE_I2S)
int shoop_fpga_configured(void) { return (Xil_In32(0xf800700c) & (1u << 2)) != 0; }
uint32_t shoop_fpga_register_base(void) { return SHOOP_PL_REG_BASE; }
uint32_t shoop_fpga_arena_base(void) { return SHOOP_PL_DDR_BASE; }
uint32_t shoop_fpga_arena_bytes(void) { return SHOOP_PL_DDR_BYTES; }
uint32_t shoop_fpga_clock_hz(void) { return SHOOP_PL_CLOCK_HZ; }

extern XScuGic xInterruptController;
static TaskHandle_t fpga_waiter;

static void fpga_irq(void *context) {
    (void)context;
    /* The fabric holds its level until the owner drains the mailbox. */
    XScuGic_Disable(&xInterruptController, SHOOP_PL_IRQ);
    BaseType_t wake = pdFALSE;
    vTaskNotifyGiveFromISR(fpga_waiter, &wake);
    portYIELD_FROM_ISR(wake);
}

int shoop_fpga_irq_start(void) {
    XScuGic_Disable(&xInterruptController, SHOOP_PL_IRQ);
    fpga_waiter = xTaskGetCurrentTaskHandle();
    XScuGic_SetPriorityTriggerType(&xInterruptController, SHOOP_PL_IRQ,
        (configMAX_API_CALL_INTERRUPT_PRIORITY + 1) << portPRIORITY_SHIFT, 1);
    return XScuGic_Connect(&xInterruptController, SHOOP_PL_IRQ, fpga_irq, NULL) == XST_SUCCESS;
}

int shoop_fpga_irq_wait(void) {
    XScuGic_Enable(&xInterruptController, SHOOP_PL_IRQ);
    return ulTaskNotifyTake(pdTRUE, 1) != 0;
}

void shoop_fpga_irq_stop(void) {
    XScuGic_Disable(&xInterruptController, SHOOP_PL_IRQ);
    XScuGic_Disconnect(&xInterruptController, SHOOP_PL_IRQ);
    (void)ulTaskNotifyTake(pdTRUE, 0);
    fpga_waiter = NULL;
}

#ifdef SHOOP_FIXTURE
void shoop_fpga_irq_pend_fixture(void) {
    XScuGic_DistWriteReg(&xInterruptController, XSCUGIC_PENDING_SET_OFFSET + 4u * (SHOOP_PL_IRQ / 32u),
                        1u << (SHOOP_PL_IRQ % 32u));
}
#endif
#endif

#ifdef SHOOP_BOARD_PHY_YT8531
static bool mdio_read(void *context, unsigned reg, uint16_t *value) {
    return XEmacPs_PhyRead(context, SHOOP_BOARD_PHY_ADDRESS, reg, value) == XST_SUCCESS;
}
static bool mdio_write(void *context, unsigned reg, uint16_t value) {
    return XEmacPs_PhyWrite(context, SHOOP_BOARD_PHY_ADDRESS, reg, value) == XST_SUCCESS;
}
static void mdio_delay(void *context, unsigned milliseconds) {
    (void)context;
    usleep(milliseconds * 1000u);
}
u32 shoop_yt8531_emacps_speed(XEmacPs *emac) {
    const struct shoop_mdio bus = {emac, mdio_read, mdio_write, mdio_delay};
    unsigned speed = shoop_yt8531_start(&bus);
    return speed ? speed : XST_FAILURE;
}
#endif

#ifdef SHOOP_BOARD_XADC
static uint32_t xadc_read(void *context, unsigned offset) {
    (void)context;
    return Xil_In32(0xf8007100u + offset);
}
static void xadc_write(void *context, unsigned offset, uint32_t value) {
    (void)context;
    Xil_Out32(0xf8007100u + offset, value);
}
#endif

int shoop_soc_temperature(struct shoop_temperature *sample) {
    *sample = (struct shoop_temperature){0};
#ifdef SHOOP_BOARD_XADC
    /* Keep the factory clock setup; unavailable if the PCAP interface clock is gated. */
    if (Xil_In32(0xf8000168u) & 1u) {
        const struct shoop_xadc_bus bus = {NULL, xadc_read, xadc_write};
        shoop_xadc_sample(&bus, sample);
    }
    return 1;
#else
    return 0;
#endif
}

static void bsp_assert(const char *file, s32 line) { vApplicationAssert(file, (unsigned long)line); }
void shoop_platform_init(void) {
    XUartPs_WriteReg(STDOUT_BASEADDRESS, XUARTPS_CR_OFFSET, XUARTPS_CR_TX_EN | XUARTPS_CR_RX_EN);
    Xil_AssertSetCallback(bsp_assert);
}

void vApplicationAssert(const char *file, unsigned long line) {
    xil_printf("FATAL assert %s:%u\r\n", file, (unsigned)line);
    portDISABLE_INTERRUPTS();
    for (;;) { __asm__ volatile("wfi"); }
}
void vApplicationMallocFailedHook(void) { vApplicationAssert("FreeRTOS heap", 0); }
__attribute__((weak)) void vApplicationTickHook(void) {}
void shoop_task_return(void) { vApplicationAssert("task returned", 0); }
void vApplicationStackOverflowHook(TaskHandle_t task, char *name) {
    (void)task;
    vApplicationAssert(name, 0);
}
void _init(void) {}
void _fini(void) {}
void outbyte(char c) { XUartPs_SendByte(STDOUT_BASEADDRESS, c); }
int _write(int fd, const void *bytes, size_t count) {
    (void)fd;
    for (size_t i = 0; i < count; ++i) outbyte(((const char *)bytes)[i]);
    return (int)count;
}
int _close(int fd) { (void)fd; errno = EBADF; return -1; }
int _isatty(int fd) { return fd >= 0 && fd <= 2; }
int _fstat(int fd, struct stat *status) {
    if (!_isatty(fd)) { errno = EBADF; return -1; }
    memset(status, 0, sizeof(*status));
    status->st_mode = S_IFCHR;
    return 0;
}
int _read(int fd, void *bytes, size_t count) {
    (void)fd; (void)bytes; (void)count; errno = EBADF; return -1;
}
off_t _lseek(int fd, off_t offset, int whence) {
    (void)fd; (void)offset; (void)whence; errno = EBADF; return -1;
}
int _getpid(void) { return 1; }
int _kill(int process, int signal) { (void)process; (void)signal; errno = ENOSYS; return -1; }
void _exit(int status) { vApplicationAssert("exit", (unsigned long)status); for (;;) {} }
void __malloc_lock(struct _reent *r) { (void)r; vTaskSuspendAll(); }
void __malloc_unlock(struct _reent *r) { (void)r; xTaskResumeAll(); }
void *_sbrk(ptrdiff_t increment) {
    extern char _heap[], _heap_end[];
    if (!c_heap_next) c_heap_next = _heap;
    if (increment < 0 || (size_t)increment > (size_t)(_heap_end - c_heap_next)) { errno = ENOMEM; return (void *)-1; }
    void *result = c_heap_next; c_heap_next += increment; return result;
}
size_t shoop_c_heap_committed(void) { extern char _heap[]; return c_heap_next ? (size_t)(c_heap_next - _heap) : 0; }

uint64_t shoop_profile_counter(void) {
    XTime value;
    XTime_GetTime(&value);
    return value;
}
