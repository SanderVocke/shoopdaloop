	.section	.text._RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,"ax",@progbits
	.globl	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.p2align	2
	.prefalign	4, .Lfunc_end366, nop
	.type	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,@function
_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak:
	.cfi_startproc
	sub	sp, sp, #224
	.cfi_def_cfa_offset 224
	stp	x29, x30, [sp, #144]
	stp	x26, x25, [sp, #160]
	stp	x24, x23, [sp, #176]
	stp	x22, x21, [sp, #192]
	stp	x20, x19, [sp, #208]
	add	x29, sp, #144
	.cfi_def_cfa w29, 80
	.cfi_offset w19, -8
	.cfi_offset w20, -16
	.cfi_offset w21, -24
	.cfi_offset w22, -32
	.cfi_offset w23, -40
	.cfi_offset w24, -48
	.cfi_offset w25, -56
	.cfi_offset w26, -64
	.cfi_offset w30, -72
	.cfi_offset w29, -80
	.cfi_remember_state
	ldr	x9, [x0, #32]
	ldr	x8, [x0, #48]
	cmp	x8, x9
	b.hi	.LBB366_70
	ldr	q2, [x1, #128]
	ldp	q0, q1, [x1, #64]
	stp	q0, q1, [sp, #64]
	ldp	q0, q1, [x1, #96]
	str	q0, [sp, #96]
	stp	q1, q2, [sp, #112]
	ldp	q0, q1, [x1]
	stp	q0, q1, [sp]
	ldp	q0, q1, [x1, #32]
	stp	q0, q1, [sp, #32]
	ldr	x13, [x0, #24]
	ldr	x14, [x0, #40]
	ldr	x9, [sp, #128]
	cbz	x9, .LBB366_4
	subs	x15, x2, #1
	b.hi	.LBB366_9
	ldp	s2, s0, [sp, #136]
	movi	d1, #0000000000000000
	b	.LBB366_10
.LBB366_4:
	mov	w21, wzr
.LBB366_5:
	ldr	x8, [x1, #128]
	cmp	x8, #3
	b.hs	.LBB366_73
	cbz	x8, .LBB366_62
	ldr	x9, [x1]
	ldr	x10, [x1, #56]
	cmp	x9, #0
	csel	x9, xzr, x10, eq
	cmp	x8, #1
	b.eq	.LBB366_63
	ldr	x8, [x1, #64]
	ldr	x10, [x1, #120]
	cmp	x8, #0
	csel	x8, xzr, x10, eq
	add	x9, x8, x9
	b	.LBB366_63
.LBB366_9:
	ldp	s2, s0, [sp, #136]
	fsub	s1, s0, s2
	ucvtf	s3, x15
	fdiv	s1, s1, s3
.LBB366_10:
	fmov	w10, s2
	and	w10, w10, #0x7fffffff
	sub	w11, w10, #2048, lsl #12
	lsr	w11, w11, #24
	sub	w10, w10, #1
	mov	w12, #8388607
	cmp	w10, w12
	mov	w10, #127
	ccmp	w11, w10, #0, hs
	cset	w10, lo
	fcmp	s2, s0
	csel	w16, wzr, w10, ne
	cmp	x9, #3
	b.hs	.LBB366_72
	mov	w21, wzr
	mov	x17, xzr
	mov	x18, sp
	add	x4, x18, x9, lsl #6
	ldr	w5, [x3, #1032]
	ldr	x6, [x3, #552]
	ldr	x3, [x3, #544]
	add	x7, x13, #32
	mov	w19, #24
	b	.LBB366_14
	.p2align	5, , 16
.LBB366_12:
	ucvtf	s3, x20
	fmul	s3, s1, s3
	fadd	s2, s2, s3
.LBB366_13:
	add	x18, x18, #64
	add	x17, x20, x17
	cmp	x18, x4
	b.eq	.LBB366_66
.LBB366_14:
	ldr	x20, [x18, #56]
	sub	x23, x2, x17
	cmp	x20, x23
	b.hi	.LBB366_67
	ldr	x9, [x18]
	cbz	x9, .LBB366_12
	cmp	x9, #1
	b.ne	.LBB366_24
	ldr	x9, [x18, #16]
	cmp	x9, x14
	b.ne	.LBB366_68
	ldr	x12, [x18, #8]
	adds	x9, x12, x20
	b.hs	.LBB366_68
	cmp	x9, x8
	mov	w11, #2305
	b.hi	.LBB366_69
	add	x9, x13, x12, lsl #2
	lsl	x10, x20, #2
	tbz	w16, #0, .LBB366_41
	cbz	x20, .LBB366_13
	sub	x10, x10, #4
	cmp	x10, #11
	b.hi	.LBB366_48
	mov	x10, x9
	b	.LBB366_60
	.p2align	5, , 16
.LBB366_24:
	ldr	w9, [x18, #16]
	cmp	w9, w5
	b.ne	.LBB366_64
	ldr	x9, [x18, #8]
	cmp	x9, x6
	mov	w11, #1281
	b.hs	.LBB366_69
	ldr	x23, [x3, x9, lsl #3]
	cbz	x23, .LBB366_64
	ldr	w9, [x18, #40]
	ldr	w10, [x23, #216]
	cmp	w9, w10
	b.ne	.LBB366_64
	ldr	w9, [x18, #44]
	ldr	x10, [x23, #112]
	cmp	x10, x9
	b.ls	.LBB366_64
	ldr	w10, [x18, #48]
	ldr	x11, [x23, #104]
	umaddl	x9, w9, w19, x11
	ldr	w11, [x9, #16]
	cmp	w11, w10
	b.ne	.LBB366_64
	ldr	x10, [x9]
	cbz	x10, .LBB366_64
	ldr	x10, [x23, #144]
	cmn	x10, #1
	b.eq	.LBB366_68
	ldr	x11, [x9, #8]
	ldp	x10, x9, [x23, #176]
	mul	x12, x9, x11
	add	x11, x9, x12
	cmp	x11, x12
	b.lo	.LBB366_71
	cmp	x11, x10
	b.hi	.LBB366_71
	cbz	x20, .LBB366_13
	ldp	x10, x22, [x18, #24]
	sub	x11, x20, #1
	madd	x11, x22, x11, x10
	cmp	x11, x10
	b.lo	.LBB366_65
	cmp	x11, x9
	mov	w11, #4097
	b.hs	.LBB366_69
	ldr	x11, [x23, #168]
	add	x11, x11, x12, lsl #2
	tbz	w16, #0, .LBB366_50
	mov	x12, x20
	.p2align	5, , 16
.LBB366_39:
	cmp	x10, x9
	b.hs	.LBB366_75
	ldr	s3, [x11, x10, lsl #2]
	fmul	s3, s2, s3
	fmov	w23, s3
	and	w23, w23, #0x7fffffff
	cmp	w23, w21
	csel	w21, w23, w21, hi
	subs	x12, x12, #1
	add	x10, x10, x22
	b.ne	.LBB366_39
	b	.LBB366_13
.LBB366_41:
	cbz	x20, .LBB366_13
	mov	x22, xzr
	subs	x10, x10, #4
	b.eq	.LBB366_47
	lsr	x11, x10, #2
	add	x11, x11, #1
	and	x11, x11, #0x7ffffffffffffffe
	sub	x12, x23, #1
	sub	x23, x23, #2
	.p2align	5, , 16
.LBB366_44:
	cmp	x2, #2
	cset	w24, lo
	add	x25, x9, x22, lsl #2
	ldp	s3, s4, [x25]
	cmp	x12, x22
	csinc	w26, w24, wzr, eq
	cmp	w26, #0
	fcsel	s2, s2, s0, ne
	fmul	s3, s2, s3
	fmov	w25, s3
	and	w25, w25, #0x7fffffff
	cmp	w25, w21
	csel	w21, w25, w21, hi
	fadd	s2, s1, s2
	cmp	x23, x22
	add	x22, x22, #2
	csinc	w24, w24, wzr, eq
	cmp	w24, #0
	fcsel	s2, s2, s0, ne
	fmul	s3, s2, s4
	fmov	w24, s3
	and	w24, w24, #0x7fffffff
	cmp	w24, w21
	csel	w21, w24, w21, hi
	fadd	s2, s1, s2
	cmp	x11, x22
	b.ne	.LBB366_44
	tbnz	w10, #2, .LBB366_13
	add	x9, x9, x22, lsl #2
.LBB366_47:
	add	x10, x17, #1
	add	x10, x10, x22
	cmp	x10, x2
	ccmp	x2, #2, #0, eq
	ldr	s3, [x9]
	fcsel	s2, s2, s0, lo
	fmul	s3, s2, s3
	fmov	w9, s3
	and	w9, w9, #0x7fffffff
	cmp	w9, w21
	csel	w21, w9, w21, hi
	fadd	s2, s1, s2
	b	.LBB366_13
.LBB366_48:
	lsr	x11, x10, #2
	add	x11, x11, #1
	cmp	x10, #60
	b.hs	.LBB366_53
	mov	x22, xzr
	b	.LBB366_57
.LBB366_50:
	sub	x12, x15, x17
	mov	x23, x20
	.p2align	5, , 16
.LBB366_51:
	cmp	x10, x9
	b.hs	.LBB366_74
	ldr	s3, [x11, x10, lsl #2]
	cmp	x12, #0
	ccmp	x2, #1, #0, eq
	fcsel	s2, s0, s2, hi
	fmul	s3, s2, s3
	fmov	w24, s3
	and	w24, w24, #0x7fffffff
	fadd	s2, s1, s2
	cmp	w24, w21
	csel	w21, w24, w21, hi
	sub	x23, x23, #1
	sub	x12, x12, #1
	add	x10, x10, x22
	cbnz	x23, .LBB366_51
	b	.LBB366_13
.LBB366_53:
	and	x23, x11, #0xc
	and	x22, x11, #0x7ffffffffffffff0
	add	x10, x9, x22, lsl #2
	dup	v3.4s, w21
	add	x21, x7, x12, lsl #2
	and	x24, x11, #0x7ffffffffffffff0
	mov	v4.16b, v3.16b
	mov	v5.16b, v3.16b
	mov	v6.16b, v3.16b
	.p2align	5, , 16
.LBB366_54:
	ldp	q7, q16, [x21]
	ldp	q17, q18, [x21, #-32]
	fmul	v17.4s, v17.4s, v2.s[0]
	fmul	v18.4s, v18.4s, v2.s[0]
	fmul	v7.4s, v7.4s, v2.s[0]
	fmul	v16.4s, v16.4s, v2.s[0]
	fabs	v17.4s, v17.4s
	fabs	v18.4s, v18.4s
	fabs	v7.4s, v7.4s
	fabs	v16.4s, v16.4s
	umax	v3.4s, v17.4s, v3.4s
	umax	v4.4s, v18.4s, v4.4s
	umax	v5.4s, v7.4s, v5.4s
	umax	v6.4s, v16.4s, v6.4s
	subs	x24, x24, #16
	add	x21, x21, #64
	b.ne	.LBB366_54
	umax	v3.4s, v3.4s, v4.4s
	umax	v3.4s, v3.4s, v5.4s
	umax	v3.4s, v3.4s, v6.4s
	umaxv	s3, v3.4s
	fmov	w21, s3
	cmp	x11, x22
	b.eq	.LBB366_13
	cbz	x23, .LBB366_60
.LBB366_57:
	and	x23, x11, #0x7ffffffffffffffc
	add	x10, x9, x23, lsl #2
	dup	v3.4s, w21
	sub	x21, x22, x23
	lsl	x22, x22, #2
	add	x12, x22, x12, lsl #2
	add	x12, x13, x12
	.p2align	5, , 16
.LBB366_58:
	ldr	q4, [x12], #16
	fmul	v4.4s, v4.4s, v2.s[0]
	fabs	v4.4s, v4.4s
	umax	v3.4s, v4.4s, v3.4s
	adds	x21, x21, #4
	b.ne	.LBB366_58
	umaxv	s3, v3.4s
	fmov	w21, s3
	cmp	x11, x23
	b.eq	.LBB366_13
.LBB366_60:
	add	x9, x9, x20, lsl #2
	.p2align	5, , 16
.LBB366_61:
	ldr	s3, [x10], #4
	fmul	s3, s2, s3
	fmov	w11, s3
	and	w11, w11, #0x7fffffff
	cmp	w11, w21
	csel	w21, w11, w21, hi
	cmp	x10, x9
	b.ne	.LBB366_61
	b	.LBB366_13
.LBB366_62:
	mov	x9, xzr
.LBB366_63:
	ldr	x8, [x0, #80]
	add	x8, x8, x9
	str	x8, [x0, #80]
	mov	w8, #65280
	orr	x11, x8, x21, lsl #32
	b	.LBB366_69
.LBB366_64:
	mov	w11, #1281
	b	.LBB366_69
.LBB366_65:
	mov	w11, #4097
	b	.LBB366_69
.LBB366_66:
	cmp	x17, x2
	b.eq	.LBB366_5
.LBB366_67:
	mov	w11, #769
	b	.LBB366_69
.LBB366_68:
	mov	w11, #2305
.LBB366_69:
	mov	x0, x11
	.cfi_def_cfa wsp, 224
	ldp	x20, x19, [sp, #208]
	ldp	x22, x21, [sp, #192]
	ldp	x24, x23, [sp, #176]
	ldp	x26, x25, [sp, #160]
	ldp	x29, x30, [sp, #144]
	add	sp, sp, #224
	.cfi_def_cfa_offset 0
	.cfi_restore w19
	.cfi_restore w20
	.cfi_restore w21
	.cfi_restore w22
	.cfi_restore w23
	.cfi_restore w24
	.cfi_restore w25
	.cfi_restore w26
	.cfi_restore w30
	.cfi_restore w29
	ret
.LBB366_70:
	.cfi_restore_state
	adrp	x3, .Lanon.4f64e9d9f78988c41611af9bb86f6254.600
	add	x3, x3, :lo12:.Lanon.4f64e9d9f78988c41611af9bb86f6254.600
	mov	x0, xzr
	mov	x1, x8
	mov	x2, x9
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB366_71:
	adrp	x3, .Lanon.4f64e9d9f78988c41611af9bb86f6254.212
	add	x3, x3, :lo12:.Lanon.4f64e9d9f78988c41611af9bb86f6254.212
	mov	x0, x12
	mov	x1, x11
	mov	x2, x10
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB366_72:
	adrp	x3, .Lanon.4f64e9d9f78988c41611af9bb86f6254.114
	add	x3, x3, :lo12:.Lanon.4f64e9d9f78988c41611af9bb86f6254.114
	mov	x0, xzr
	mov	x1, x9
	mov	w2, #2
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB366_73:
	adrp	x3, .Lanon.4f64e9d9f78988c41611af9bb86f6254.578
	add	x3, x3, :lo12:.Lanon.4f64e9d9f78988c41611af9bb86f6254.578
	mov	x0, xzr
	mov	x1, x8
	mov	w2, #2
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB366_74:
	adrp	x2, .Lanon.4f64e9d9f78988c41611af9bb86f6254.132
	add	x2, x2, :lo12:.Lanon.4f64e9d9f78988c41611af9bb86f6254.132
	mov	x0, x10
	mov	x1, x9
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.LBB366_75:
	adrp	x2, .Lanon.4f64e9d9f78988c41611af9bb86f6254.133
	add	x2, x2, :lo12:.Lanon.4f64e9d9f78988c41611af9bb86f6254.133
	mov	x0, x10
	mov	x1, x9
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.Lfunc_end366:
	.size	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak, .Lfunc_end366-_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.cfi_endproc

