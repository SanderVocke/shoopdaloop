.syntax unified
.arm
.text
.align 2
.global shoop_exception_mask
.type shoop_exception_mask, %function
shoop_exception_mask:
    cpsid ifa
    isb
    bx lr
.size shoop_exception_mask, .-shoop_exception_mask

.global shoop_exception_halt
.type shoop_exception_halt, %function
shoop_exception_halt:
    cpsid ifa
    dsb
1:  wfi
    b 1b
.size shoop_exception_halt, .-shoop_exception_halt

.global shoop_exception_registers
.type shoop_exception_registers, %function
shoop_exception_registers:
    mrs r1, cpsr
    str r1, [r0, #0]
    mrs r1, spsr
    str r1, [r0, #4]
    mrc p15, 0, r1, c5, c0, 0
    str r1, [r0, #8]
    mrc p15, 0, r1, c6, c0, 0
    str r1, [r0, #12]
    mrc p15, 0, r1, c5, c0, 1
    str r1, [r0, #16]
    mrc p15, 0, r1, c6, c0, 2
    str r1, [r0, #20]
    bx lr
.size shoop_exception_registers, .-shoop_exception_registers
.section .note.GNU-stack,"",%progbits
