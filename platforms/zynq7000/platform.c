#include "FreeRTOS.h"
#include "task.h"
#include "xscugic.h"
#include "xscutimer.h"
#include "xemacps.h"
#include "xuartps.h"
#include "xil_printf.h"
#include "xil_assert.h"
#include "netif/xtopology.h"
#include <sys/stat.h>
#include <errno.h>
#include <string.h>
static char *c_heap_next;

XScuGic_Config XScuGic_ConfigTable[] = {{ .DeviceId = 0, .CpuBaseAddress = 0xf8f00100, .DistBaseAddress = 0xf8f01000 }};
XScuTimer_Config XScuTimer_ConfigTable[] = {{ .DeviceId = 0, .BaseAddr = 0xf8f00600 }};
XEmacPs_Config XEmacPs_ConfigTable[] = {{ .DeviceId = 0, .BaseAddress = SHOOP_GEM_BASE, .S1GDiv0 = SHOOP_GEM_1000_DIV0, .S1GDiv1 = SHOOP_GEM_1000_DIV1, .S100MDiv0 = SHOOP_GEM_100_DIV0, .S100MDiv1 = SHOOP_GEM_100_DIV1, .S10MDiv0 = SHOOP_GEM_10_DIV0, .S10MDiv1 = SHOOP_GEM_10_DIV1 }};
struct xtopology_t xtopology[] = {{ .emac_baseaddr = SHOOP_GEM_BASE, .emac_type = xemac_type_emacps, .scugic_baseaddr = 0xf8f01000, .scugic_emac_intr = SHOOP_GEM_IRQ }};
int xtopology_n_emacs = 1;

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
