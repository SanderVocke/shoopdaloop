	.section	.text._RNvMs0_NtCs3b9Kxd3kAw0_11shoop_loops8resolverNtB5_8Resolver21prerouting_transition,"ax",@progbits
	.globl	_RNvMs0_NtCs3b9Kxd3kAw0_11shoop_loops8resolverNtB5_8Resolver21prerouting_transition
	.p2align	2
	.prefalign	4, .Lfunc_end224, nop
	.type	_RNvMs0_NtCs3b9Kxd3kAw0_11shoop_loops8resolverNtB5_8Resolver21prerouting_transition,@function
_RNvMs0_NtCs3b9Kxd3kAw0_11shoop_loops8resolverNtB5_8Resolver21prerouting_transition:
	.cfi_startproc
	sub	sp, sp, #80
	.cfi_def_cfa_offset 80
	stp	x29, x30, [sp, #64]
	add	x29, sp, #64
	.cfi_def_cfa w29, 16
	.cfi_offset w30, -8
	.cfi_offset w29, -16
	.cfi_remember_state
	ldr	x9, [x0, #328]
	cbz	x9, .LBB224_6
	ldr	x11, [x0, #320]
	cmp	x9, #1
	b.ne	.LBB224_3
	mov	x10, xzr
	b	.LBB224_5
.LBB224_3:
	mov	x10, xzr
	mov	x12, x9
	.p2align	5, , 16
.LBB224_4:
	lsr	x13, x12, #1
	add	x14, x13, x10
	add	x15, x11, x14, lsl #5
	ldp	w16, w15, [x15]
	cmp	w16, w1
	cset	w16, hi
	cmp	w15, w2
	cset	w15, hi
	csel	w15, w16, w15, eq
	cmp	w15, #0
	csel	x10, x10, x14, ne
	sub	x12, x12, x13
	cmp	x12, #1
	b.hi	.LBB224_4
.LBB224_5:
	add	x12, x11, x10, lsl #5
	ldp	w11, w13, [x12]
	cmp	w11, w1
	cset	w14, hi
	csinv	w14, w14, wzr, hs
	cmp	w13, w2
	cset	w13, hi
	csinv	w13, w13, wzr, hs
	csel	w13, w14, w13, eq
	tst	w13, #0xff
	b.eq	.LBB224_8
.LBB224_6:
	mov	w11, #2
.LBB224_7:
	strb	w11, [x8, #1]
	mov	w9, #254
	strb	w9, [x8]
	.cfi_def_cfa wsp, 80
	ldp	x29, x30, [sp, #64]
	add	sp, sp, #80
	.cfi_def_cfa_offset 0
	.cfi_restore w30
	.cfi_restore w29
	ret
.LBB224_8:
	.cfi_restore_state
	.cfi_remember_state
	cmp	x10, x9
	b.hs	.LBB224_43
	ldrb	w9, [x12, #24]
	cmp	w9, #2
	b.eq	.LBB224_7
	ldrb	w9, [x0, #1075]
	tbnz	w9, #0, .LBB224_23
	ldrb	w9, [x0, #1074]
	tbnz	w9, #0, .LBB224_23
	ldr	x9, [x0]
	cmp	x9, #0
	cset	w9, eq
	ldrb	w10, [x0, #1078]
	orr	w9, w9, w10
	tst	w9, #0x1
	mov	w9, #704
	mov	w10, #584
	csel	x9, x10, x9, ne
	add	x9, x0, x9
	ldr	x12, [x9, #16]
	cbz	x12, .LBB224_41
	mov	x11, xzr
	ldr	x10, [x9, #8]
	lsl	x12, x12, #7
	ldr	x9, [x0, #1056]
	b	.LBB224_15
	.p2align	5, , 16
.LBB224_14:
	add	x11, x11, #128
	cmp	x12, x11
	b.eq	.LBB224_41
.LBB224_15:
	add	x13, x10, x11
	ldrb	w14, [x13, #120]
	cmp	w14, #2
	b.eq	.LBB224_14
	ldrb	w13, [x13]
	cmp	w13, #255
	b.ne	.LBB224_14
	add	x13, x10, x11
	ldr	w14, [x13, #52]
	cmp	w14, w2
	b.ne	.LBB224_14
	ldr	w13, [x13, #48]
	cmp	w13, w1
	b.ne	.LBB224_14
	add	x14, x10, x11
	ldr	x13, [x14, #96]
	cmp	x13, x9
	b.hi	.LBB224_14
	ldr	w14, [x14, #72]
	cbz	w14, .LBB224_14
	sub	x14, x12, #128
	add	x15, x10, x11
	cmp	x14, x11
	b.eq	.LBB224_39
	add	x10, x15, #128
	sub	x11, x12, x11
	sub	x12, x11, #128
	ldr	x16, [x15, #56]
	ldr	x14, [x15, #104]
	mov	x11, sp
	add	x11, x11, #32
	lsr	x12, x12, #7
	b	.LBB224_27
.LBB224_23:
	mov	w9, #255
	strb	w9, [x8]
	.cfi_def_cfa wsp, 80
	ldp	x29, x30, [sp, #64]
	add	sp, sp, #80
	.cfi_def_cfa_offset 0
	.cfi_restore w30
	.cfi_restore w29
	ret
.LBB224_24:
	.cfi_restore_state
	.cfi_remember_state
	mov	x15, sp
	mov	x17, x14
.LBB224_25:
	ldp	x13, x16, [x15, #8]
	mov	x14, x17
	ldr	x15, [x15, #24]
.LBB224_26:
	subs	x12, x12, #1
	add	x10, x10, #128
	b.eq	.LBB224_38
.LBB224_27:
	ldrb	w17, [x10, #120]
	cmp	w17, #2
	b.eq	.LBB224_26
	ldrb	w17, [x10]
	cmp	w17, #255
	b.ne	.LBB224_26
	ldr	w17, [x10, #52]
	cmp	w17, w2
	b.ne	.LBB224_26
	ldr	w17, [x10, #48]
	cmp	w17, w1
	b.ne	.LBB224_26
	ldr	x18, [x10, #96]
	cmp	x18, x9
	b.hi	.LBB224_26
	ldr	w17, [x10, #72]
	cbz	w17, .LBB224_26
	stp	x13, x16, [sp, #8]
	ldr	x17, [x10, #104]
	stp	x15, x17, [sp, #24]
	ldr	x15, [x10, #56]
	stp	x18, x15, [sp, #40]
	str	x10, [sp, #56]
	cmp	x14, x17
	cset	w0, ls
	b.ne	.LBB224_37
	cmp	x13, x18
	cset	w0, ls
	b.ne	.LBB224_37
	cmp	x15, x16
	b.ls	.LBB224_24
	mov	x17, x14
	mov	x15, x11
	b	.LBB224_25
.LBB224_37:
	mov	x15, x11
	cbz	w0, .LBB224_25
	b	.LBB224_24
.LBB224_38:
	cbz	x15, .LBB224_41
.LBB224_39:
	ldr	x9, [x15, #104]
	cmp	x9, #1
	b.ne	.LBB224_41
	ldur	q0, [x15, #17]
	ldr	q1, [x15, #32]
	str	q0, [sp]
	ldrb	w9, [x15, #16]
	stur	q1, [sp, #15]
	b	.LBB224_42
.LBB224_41:
	mov	w9, #255
.LBB224_42:
	ldr	q0, [sp]
	ldur	q1, [sp, #15]
	stur	q0, [x8, #1]
	strb	w9, [x8]
	str	q1, [x8, #16]
	.cfi_def_cfa wsp, 80
	ldp	x29, x30, [sp, #64]
	add	sp, sp, #80
	.cfi_def_cfa_offset 0
	.cfi_restore w30
	.cfi_restore w29
	ret
.LBB224_43:
	.cfi_restore_state
	adrp	x2, .Lanon.b3756854fe5b5adef27dec529f896d7d.156
	add	x2, x2, :lo12:.Lanon.b3756854fe5b5adef27dec529f896d7d.156
	mov	x0, x10
	mov	x1, x9
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.Lfunc_end224:
