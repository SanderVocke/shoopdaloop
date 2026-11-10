
	.section	.text._RNvXs4_NtCsjP1tzkscQDQ_15shoop_rt_engine5cellsNtB5_8ObserverNtNtCs3b9Kxd3kAw0_11shoop_loops7runtime16BoundaryObserver8boundary,"ax",@progbits
	.globl	_RNvXs4_NtCsjP1tzkscQDQ_15shoop_rt_engine5cellsNtB5_8ObserverNtNtCs3b9Kxd3kAw0_11shoop_loops7runtime16BoundaryObserver8boundary
	.p2align	2
	.prefalign	4, .Lfunc_end413, nop
	.type	_RNvXs4_NtCsjP1tzkscQDQ_15shoop_rt_engine5cellsNtB5_8ObserverNtNtCs3b9Kxd3kAw0_11shoop_loops7runtime16BoundaryObserver8boundary,@function
_RNvXs4_NtCsjP1tzkscQDQ_15shoop_rt_engine5cellsNtB5_8ObserverNtNtCs3b9Kxd3kAw0_11shoop_loops7runtime16BoundaryObserver8boundary:
	.cfi_startproc
	sub	sp, sp, #112
	.cfi_def_cfa_offset 112
	stp	x29, x30, [sp, #16]
	stp	x28, x27, [sp, #32]
	stp	x26, x25, [sp, #48]
	stp	x24, x23, [sp, #64]
	stp	x22, x21, [sp, #80]
	stp	x20, x19, [sp, #96]
	add	x29, sp, #16
	.cfi_def_cfa w29, 96
	.cfi_offset w19, -8
	.cfi_offset w20, -16
	.cfi_offset w21, -24
	.cfi_offset w22, -32
	.cfi_offset w23, -40
	.cfi_offset w24, -48
	.cfi_offset w25, -56
	.cfi_offset w26, -64
	.cfi_offset w27, -72
	.cfi_offset w28, -80
	.cfi_offset w30, -88
	.cfi_offset w29, -96
	.cfi_remember_state
	cbz	x3, .LBB413_34
	mov	w13, #152
	ldr	x10, [x0]
	ldp	x11, x8, [x0, #88]
	str	x8, [sp, #8]
	ldp	x12, x9, [x0, #104]
	mul	x13, x3, x13
	ldr	x14, [x0, #120]
	add	x15, x2, #72
	b	.LBB413_4
	.p2align	5, , 16
.LBB413_2:
	add	w8, w30, #1
	add	x16, x17, #712
	and	w0, w8, #0xff
	stlr	w0, [x16]
	add	x16, x17, #716
	cmp	w0, w8
	csel	w8, wzr, w5, ne
	stlr	w8, [x16]
	add	x8, x17, #708
	stlr	w25, [x8]
	add	x8, x17, #721
	stlrb	w28, [x8]
	cmp	w25, #2
	csel	x8, x27, x26, eq
	add	x16, x17, #672
	stlr	x8, [x16]
	add	x8, x17, #680
	stlr	x23, [x8]
	add	x8, x17, #664
	stlr	x6, [x8]
	add	x8, x17, #688
	stlr	x7, [x8]
	add	x8, x17, #632
	stlr	x14, [x8]
	add	x8, x17, #640
	stlr	x1, [x8]
	cmp	x24, #2
	csel	x8, xzr, x20, eq
	csel	x16, xzr, x21, eq
	csel	w0, wzr, w22, eq
	add	x2, x17, #648
	stlr	x8, [x2]
	add	x8, x17, #656
	stlr	x16, [x8]
	add	x8, x17, #720
	stlrb	w0, [x8]
	add	x8, x18, #2
	add	x16, x17, #624
	stlr	x8, [x16]
.LBB413_3:
	add	x15, x15, #152
	subs	x13, x13, #152
	b.eq	.LBB413_34
.LBB413_4:
	ldur	w0, [x15, #-12]
	ldr	x16, [x10, #40]
	cmp	x16, x0
	b.ls	.LBB413_3
	ldr	x8, [sp, #8]
	cmp	x8, x0
	b.ls	.LBB413_36
	ldur	w19, [x15, #-16]
	mov	w8, #12
	umaddl	x17, w0, w8, x11
	ldur	x16, [x17, #4]
	mov	x8, #1095216660480
	bics	xzr, x8, x16
	b.ne	.LBB413_8
	orr	x5, x5, #0xff00000000
	b	.LBB413_9
	.p2align	5, , 16
.LBB413_8:
	ldr	w17, [x17]
	orr	x18, x5, #0xff00000000
	cmp	w17, w19
	csel	w4, w19, w4, eq
	csel	x5, x16, x18, eq
.LBB413_9:
	cmp	x9, x0
	b.ls	.LBB413_35
	ldrb	w25, [x15, #59]
	ldp	x6, x26, [x15, #-8]
	ldp	x23, x7, [x15, #8]
	ldr	x27, [x15, #32]
	ldrb	w28, [x15, #56]
	ldp	x3, x16, [x15, #-72]
	ldur	x24, [x15, #-56]
	ldp	x20, x21, [x15, #-40]
	ldurb	w22, [x15, #-24]
	add	x18, x12, x0, lsl #6
	ubfx	x30, x5, #32, #8
	ldr	x17, [x10, #32]
	cbnz	w25, .LBB413_26
	cmp	x24, #2
	b.ne	.LBB413_26
	cmp	x16, x1
	csel	w2, wzr, w3, ne
	tbnz	w2, #0, .LBB413_29
	ldrb	w2, [x18, #56]
	cmp	w2, #2
	b.eq	.LBB413_29
	ldr	w8, [x18, #52]
	cmp	w8, w19
	b.ne	.LBB413_29
	ldr	x8, [x18, #8]
	cmp	x8, x26
	b.ne	.LBB413_29
	ldr	x8, [x18, #16]
	cmp	x8, x23
	b.ne	.LBB413_29
	ldr	x8, [x18, #24]
	cmp	x8, x6
	b.ne	.LBB413_29
	cmp	w2, w28
	b.ne	.LBB413_29
	ldr	x8, [x18, #32]
	cmp	x8, x7
	b.ne	.LBB413_29
	ldr	x8, [x18]
	cmp	x8, x14
	b.ne	.LBB413_29
	ldrb	w2, [x18, #48]
	cmp	w30, #255
	b.eq	.LBB413_28
	cmp	w2, #255
	b.eq	.LBB413_28
	ldr	w8, [x18, #40]
	cmp	w8, w4
	b.ne	.LBB413_29
	cmp	w2, w30
	b.ne	.LBB413_29
	ldr	w8, [x18, #44]
	cmp	w8, w5
	b.eq	.LBB413_3
	b	.LBB413_29
	.p2align	5, , 16
.LBB413_26:
	ldrb	w2, [x18, #56]
	cmp	w2, #2
	b.eq	.LBB413_30
	mov	w8, #2
	strb	w8, [x18, #56]
	b	.LBB413_30
.LBB413_28:
	and	w8, w2, w30
	cmp	w8, #255
	b.eq	.LBB413_3
	.p2align	5, , 16
.LBB413_29:
	stp	x14, x26, [x18]
	stp	x23, x6, [x18, #16]
	str	x7, [x18, #32]
	str	w4, [x18, #40]
	stur	x5, [x18, #44]
	str	w19, [x18, #52]
	strb	w28, [x18, #56]
.LBB413_30:
	mov	w8, #728
	umaddl	x17, w0, w8, x17
	add	x8, x17, #624
	mov	w18, #1
	ldaddal	x18, x18, [x8]
	add	x0, x17, #704
	ldar	w8, [x0]
	cmp	w8, w19
	b.eq	.LBB413_32
	add	x8, x17, #696
	stlr	xzr, [x8]
	stlr	w19, [x0]
.LBB413_32:
	cmp	x16, x1
	csel	w8, wzr, w3, ne
	tbz	w8, #0, .LBB413_2
	add	x8, x17, #696
	mov	w16, #1
	ldaddal	x16, x8, [x8]
	b	.LBB413_2
.LBB413_34:
	.cfi_def_cfa wsp, 112
	ldp	x20, x19, [sp, #96]
	ldp	x22, x21, [sp, #80]
	ldp	x24, x23, [sp, #64]
	ldp	x26, x25, [sp, #48]
	ldp	x28, x27, [sp, #32]
	ldp	x29, x30, [sp, #16]
	add	sp, sp, #112
	.cfi_def_cfa_offset 0
	.cfi_restore w19
	.cfi_restore w20
	.cfi_restore w21
	.cfi_restore w22
	.cfi_restore w23
	.cfi_restore w24
	.cfi_restore w25
	.cfi_restore w26
	.cfi_restore w27
	.cfi_restore w28
	.cfi_restore w30
	.cfi_restore w29
	ret
.LBB413_35:
	.cfi_restore_state
	adrp	x2, .Lanon.9fa11758a76539974994d613721179cf.850
	add	x2, x2, :lo12:.Lanon.9fa11758a76539974994d613721179cf.850
	mov	x1, x9
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.LBB413_36:
	adrp	x2, .Lanon.9fa11758a76539974994d613721179cf.849
	add	x2, x2, :lo12:.Lanon.9fa11758a76539974994d613721179cf.849
	ldr	x1, [sp, #8]
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.Lfunc_end413:
	.size	_RNvXs4_NtCsjP1tzkscQDQ_15shoop_rt_engine5cellsNtB5_8ObserverNtNtCs3b9Kxd3kAw0_11shoop_loops7runtime16BoundaryObserver8boundary, .Lfunc_end413-_RNvXs4_NtCsjP1tzkscQDQ_15shoop_rt_engine5cellsNtB5_8ObserverNtNtCs3b9Kxd3kAw0_11shoop_loops7runtime16BoundaryObserver8boundary
	.cfi_endproc
