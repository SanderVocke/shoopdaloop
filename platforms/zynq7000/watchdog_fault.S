.syntax unified
.arm
.section .text.shoop_watchdog_inject, "ax", %progbits
.global shoop_watchdog_inject
.type shoop_watchdog_inject, %function
shoop_watchdog_inject:
    cmp r0, #5
    bne 1f
    udf #0
1:  cpsid ifa
2:  wfi
    b 2b
.size shoop_watchdog_inject, .-shoop_watchdog_inject
.section .note.GNU-stack, "", %progbits
