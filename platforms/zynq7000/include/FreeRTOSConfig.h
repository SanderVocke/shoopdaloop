#ifndef SHOOP_FREERTOSCONFIG_H
#define SHOOP_FREERTOSCONFIG_H
#include "xparameters.h"
#define configUSE_PREEMPTION 1
#define configUSE_TIME_SLICING 1
#define configIDLE_SHOULD_YIELD 1
#define configCPU_CLOCK_HZ XPAR_CPU_CORTEXA9_0_CPU_CLK_FREQ_HZ
#define configTICK_RATE_HZ 1000
#define configMAX_PRIORITIES 8
#define configMINIMAL_STACK_SIZE 1024
#define configTOTAL_HEAP_SIZE (16 * 1024 * 1024)
#define configMAX_TASK_NAME_LEN 16
#define configUSE_16_BIT_TICKS 0
#define configUSE_TASK_FPU_SUPPORT 2
#define configUSE_MUTEXES 1
#define configUSE_RECURSIVE_MUTEXES 1
#define configUSE_COUNTING_SEMAPHORES 1
#define configUSE_TASK_NOTIFICATIONS 1
#define configUSE_NEWLIB_REENTRANT 1
#define configUSE_TRACE_FACILITY 1
#define configCHECK_FOR_STACK_OVERFLOW 2
#define configUSE_MALLOC_FAILED_HOOK 1
#define configUSE_IDLE_HOOK 0
#define configUSE_TICK_HOOK 0
#define configUSE_TIMERS 1
#define configTIMER_TASK_PRIORITY 4
#define configTIMER_QUEUE_LENGTH 32
#define configTIMER_TASK_STACK_DEPTH 2048
#define configQUEUE_REGISTRY_SIZE 16
#define configGENERATE_RUN_TIME_STATS 0
#define configSUPPORT_STATIC_ALLOCATION 0
#define configSUPPORT_DYNAMIC_ALLOCATION 1
#define configINTERRUPT_CONTROLLER_BASE_ADDRESS 0xf8f01000
#define configINTERRUPT_CONTROLLER_CPU_INTERFACE_OFFSET (-0xf00)
#define configUNIQUE_INTERRUPT_PRIORITIES 32
#define configMAX_API_CALL_INTERRUPT_PRIORITY 18
#define INCLUDE_vTaskDelay 1
#define INCLUDE_vTaskDelayUntil 1
#define INCLUDE_vTaskDelete 1
#define INCLUDE_vTaskSuspend 1
#define INCLUDE_uxTaskGetStackHighWaterMark 1
#define INCLUDE_xTaskGetCurrentTaskHandle 1
#define INCLUDE_xSemaphoreGetMutexHolder 1
#define INCLUDE_xTimerPendFunctionCall 1
void vApplicationAssert(const char *, unsigned long);
void shoop_task_return(void);
#define configTASK_RETURN_ADDRESS shoop_task_return
void FreeRTOS_SetupTickInterrupt(void);
void FreeRTOS_ClearTickInterrupt(void);
#define configASSERT(x) do { if (!(x)) vApplicationAssert(__FILE__, __LINE__); } while (0)
#define configSETUP_TICK_INTERRUPT() FreeRTOS_SetupTickInterrupt()
#define configCLEAR_TICK_INTERRUPT() FreeRTOS_ClearTickInterrupt()
#endif
