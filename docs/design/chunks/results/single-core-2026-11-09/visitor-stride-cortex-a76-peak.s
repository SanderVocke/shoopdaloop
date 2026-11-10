
	.section	.text._RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,"ax",@progbits
	.globl	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.p2align	2
	.prefalign	4, .Lfunc_end365, nop
	.type	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,@function
_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak:
	.cfi_startproc
	sub	sp, sp, #288
	.cfi_def_cfa_offset 288
	stp	x29, x30, [sp, #192]
	stp	x28, x27, [sp, #208]
	stp	x26, x25, [sp, #224]
	stp	x24, x23, [sp, #240]
	stp	x22, x21, [sp, #256]
	stp	x20, x19, [sp, #272]
	add	x29, sp, #192
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
	ldr	x9, [x0, #32]
	ldr	x8, [x0, #48]
	cmp	x8, x9
	b.hi	.LBB365_105
	ldr	q2, [x1, #128]
	ldp	q0, q1, [x1, #64]
	stp	q0, q1, [sp, #96]
	ldp	q0, q1, [x1, #96]
	str	q0, [sp, #128]
	stp	q1, q2, [sp, #144]
	ldp	q0, q1, [x1]
	stp	q0, q1, [sp, #32]
	ldp	q0, q1, [x1, #32]
	stp	q0, q1, [sp, #64]
	ldr	x13, [x0, #24]
	ldr	x14, [x0, #40]
	ldr	x9, [sp, #160]
	cbz	x9, .LBB365_4
	subs	x15, x2, #1
	b.hi	.LBB365_9
	ldp	s2, s0, [sp, #168]
	movi	d1, #0000000000000000
	b	.LBB365_10
.LBB365_4:
	mov	w23, wzr
.LBB365_5:
	ldr	x8, [x1, #128]
	cmp	x8, #3
	b.hs	.LBB365_108
	cbz	x8, .LBB365_97
	ldr	x9, [x1]
	ldr	x10, [x1, #56]
	cmp	x9, #0
	csel	x9, xzr, x10, eq
	cmp	x8, #1
	b.eq	.LBB365_98
	ldr	x8, [x1, #64]
	ldr	x10, [x1, #120]
	cmp	x8, #0
	csel	x8, xzr, x10, eq
	add	x9, x8, x9
	b	.LBB365_98
.LBB365_9:
	ldp	s2, s0, [sp, #168]
	fsub	s1, s0, s2
	ucvtf	s3, x15
	fdiv	s1, s1, s3
.LBB365_10:
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
	b.hs	.LBB365_107
	mov	w23, wzr
	mov	x17, xzr
	add	x18, sp, #32
	add	x4, x18, x9, lsl #6
	ldr	w5, [x3, #1032]
	ldr	x6, [x3, #552]
	ldr	x3, [x3, #544]
	add	x7, x13, #32
	mov	w19, #24
	mov	w28, #16
	b	.LBB365_14
	.p2align	5, , 16
.LBB365_12:
	ucvtf	s3, x22
	fmul	s3, s1, s3
	fadd	s2, s2, s3
.LBB365_13:
	add	x18, x18, #64
	add	x17, x22, x17
	cmp	x18, x4
	b.eq	.LBB365_101
.LBB365_14:
	ldr	x22, [x18, #56]
	sub	x25, x2, x17
	cmp	x22, x25
	b.hi	.LBB365_102
	ldr	x9, [x18]
	cbz	x9, .LBB365_12
	cmp	x9, #1
	b.ne	.LBB365_24
	ldr	x9, [x18, #16]
	cmp	x9, x14
	b.ne	.LBB365_103
	ldr	x24, [x18, #8]
	adds	x9, x24, x22
	b.hs	.LBB365_103
	cmp	x9, x8
	mov	w12, #2305
	b.hi	.LBB365_104
	add	x9, x13, x24, lsl #2
	lsl	x10, x22, #2
	tbz	w16, #0, .LBB365_44
	cbz	x22, .LBB365_13
	sub	x10, x10, #4
	cmp	x10, #11
	b.hi	.LBB365_51
	mov	x10, x9
	b	.LBB365_63
	.p2align	5, , 16
.LBB365_24:
	ldr	w9, [x18, #16]
	cmp	w9, w5
	b.ne	.LBB365_99
	ldr	x9, [x18, #8]
	cmp	x9, x6
	mov	w12, #1281
	b.hs	.LBB365_104
	ldr	x21, [x3, x9, lsl #3]
	cbz	x21, .LBB365_99
	ldr	w9, [x18, #40]
	ldr	w10, [x21, #216]
	cmp	w9, w10
	b.ne	.LBB365_99
	ldr	w9, [x18, #44]
	ldr	x10, [x21, #112]
	cmp	x10, x9
	b.ls	.LBB365_99
	ldr	w10, [x18, #48]
	ldr	x11, [x21, #104]
	umaddl	x9, w9, w19, x11
	ldr	w11, [x9, #16]
	cmp	w11, w10
	b.ne	.LBB365_99
	ldr	x10, [x9]
	cbz	x10, .LBB365_99
	ldr	x10, [x21, #144]
	cmn	x10, #1
	b.eq	.LBB365_103
	ldr	x10, [x9, #8]
	ldp	x9, x11, [x21, #176]
	mul	x10, x11, x10
	add	x12, x11, x10
	cmp	x12, x10
	b.lo	.LBB365_106
	cmp	x12, x9
	b.hi	.LBB365_106
	cbz	x22, .LBB365_13
	ldp	x9, x25, [x18, #24]
	sub	x12, x22, #1
	madd	x12, x25, x12, x9
	cmp	x12, x9
	b.lo	.LBB365_100
	cmp	x12, x11
	mov	w12, #4097
	b.hs	.LBB365_104
	ldr	x24, [x21, #168]
	add	x26, x24, x10, lsl #2
	tbz	w16, #0, .LBB365_53
	cmp	x25, #1
	b.eq	.LBB365_65
	cmp	x25, #2
	b.ne	.LBB365_69
	sub	x11, x11, x9
	lsr	x25, x11, #1
	cmp	x25, x22
	csel	x30, x25, x22, lo
	lsl	x12, x30, #1
	cmp	x12, x11
	b.hi	.LBB365_110
	add	x26, x26, x9, lsl #2
	cbz	x25, .LBB365_93
	sub	x21, x12, #2
	cmp	x21, #7
	str	x26, [sp, #24]
	b.hi	.LBB365_74
	mov	x28, x12
	mov	x27, x26
	b	.LBB365_91
.LBB365_44:
	cbz	x22, .LBB365_13
	mov	x21, xzr
	subs	x10, x10, #4
	b.eq	.LBB365_50
	lsr	x11, x10, #2
	add	x11, x11, #1
	and	x11, x11, #0x7ffffffffffffffe
	sub	x12, x25, #1
	sub	x24, x25, #2
	.p2align	5, , 16
.LBB365_47:
	cmp	x2, #2
	cset	w20, lo
	add	x25, x9, x21, lsl #2
	ldp	s3, s4, [x25]
	cmp	x12, x21
	csinc	w26, w20, wzr, eq
	cmp	w26, #0
	fcsel	s2, s2, s0, ne
	fmul	s3, s2, s3
	fmov	w25, s3
	and	w25, w25, #0x7fffffff
	cmp	w25, w23
	csel	w23, w25, w23, hi
	fadd	s2, s1, s2
	cmp	x24, x21
	add	x21, x21, #2
	csinc	w20, w20, wzr, eq
	cmp	w20, #0
	fcsel	s2, s2, s0, ne
	fmul	s3, s2, s4
	fmov	w20, s3
	and	w20, w20, #0x7fffffff
	cmp	w20, w23
	csel	w23, w20, w23, hi
	fadd	s2, s1, s2
	cmp	x11, x21
	b.ne	.LBB365_47
	tbnz	w10, #2, .LBB365_13
	add	x9, x9, x21, lsl #2
.LBB365_50:
	add	x10, x17, #1
	add	x10, x10, x21
	cmp	x10, x2
	ccmp	x2, #2, #0, eq
	ldr	s3, [x9]
	fcsel	s2, s2, s0, lo
	fmul	s3, s2, s3
	fmov	w9, s3
	and	w9, w9, #0x7fffffff
	cmp	w9, w23
	csel	w23, w9, w23, hi
	fadd	s2, s1, s2
	b	.LBB365_13
.LBB365_51:
	lsr	x11, x10, #2
	add	x11, x11, #1
	cmp	x10, #60
	b.hs	.LBB365_56
	mov	x12, xzr
	b	.LBB365_60
.LBB365_53:
	sub	x10, x15, x17
	mov	x12, x22
	.p2align	5, , 16
.LBB365_54:
	cmp	x9, x11
	b.hs	.LBB365_111
	ldr	s3, [x26, x9, lsl #2]
	cmp	x10, #0
	ccmp	x2, #1, #0, eq
	fcsel	s2, s0, s2, hi
	fmul	s3, s2, s3
	fmov	w21, s3
	and	w21, w21, #0x7fffffff
	fadd	s2, s1, s2
	cmp	w21, w23
	csel	w23, w21, w23, hi
	sub	x12, x12, #1
	sub	x10, x10, #1
	add	x9, x9, x25
	cbnz	x12, .LBB365_54
	b	.LBB365_13
.LBB365_56:
	and	x21, x11, #0xc
	and	x12, x11, #0x7ffffffffffffff0
	add	x10, x9, x12, lsl #2
	dup	v3.4s, w23
	add	x23, x7, x24, lsl #2
	and	x25, x11, #0x7ffffffffffffff0
	mov	v4.16b, v3.16b
	mov	v5.16b, v3.16b
	mov	v6.16b, v3.16b
	.p2align	5, , 16
.LBB365_57:
	ldp	q7, q16, [x23]
	ldp	q17, q18, [x23, #-32]
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
	subs	x25, x25, #16
	add	x23, x23, #64
	b.ne	.LBB365_57
	umax	v3.4s, v3.4s, v4.4s
	umax	v3.4s, v3.4s, v5.4s
	umax	v3.4s, v3.4s, v6.4s
	umaxv	s3, v3.4s
	fmov	w23, s3
	cmp	x11, x12
	b.eq	.LBB365_13
	cbz	x21, .LBB365_63
.LBB365_60:
	and	x21, x11, #0x7ffffffffffffffc
	add	x10, x9, x21, lsl #2
	dup	v3.4s, w23
	sub	x23, x12, x21
	lsl	x12, x12, #2
	add	x12, x12, x24, lsl #2
	add	x12, x13, x12
	.p2align	5, , 16
.LBB365_61:
	ldr	q4, [x12], #16
	fmul	v4.4s, v4.4s, v2.s[0]
	fabs	v4.4s, v4.4s
	umax	v3.4s, v4.4s, v3.4s
	adds	x23, x23, #4
	b.ne	.LBB365_61
	umaxv	s3, v3.4s
	fmov	w23, s3
	cmp	x11, x21
	b.eq	.LBB365_13
.LBB365_63:
	add	x9, x9, x22, lsl #2
	.p2align	5, , 16
.LBB365_64:
	ldr	s3, [x10], #4
	fmul	s3, s2, s3
	fmov	w11, s3
	and	w11, w11, #0x7fffffff
	cmp	w11, w23
	csel	w23, w11, w23, hi
	cmp	x10, x9
	b.ne	.LBB365_64
	b	.LBB365_13
.LBB365_65:
	adds	x12, x9, x22
	b.hs	.LBB365_109
	cmp	x12, x11
	b.hi	.LBB365_109
	add	x11, x26, x9, lsl #2
	lsl	x12, x22, #2
	sub	x21, x12, #4
	cmp	x21, #11
	b.hi	.LBB365_72
	mov	x25, x11
	b	.LBB365_83
.LBB365_69:
	mov	x10, x22
	.p2align	5, , 16
.LBB365_70:
	cmp	x9, x11
	b.hs	.LBB365_112
	ldr	s3, [x26, x9, lsl #2]
	fmul	s3, s2, s3
	fmov	w12, s3
	and	w12, w12, #0x7fffffff
	cmp	w12, w23
	csel	w23, w12, w23, hi
	subs	x10, x10, #1
	add	x9, x9, x25
	b.ne	.LBB365_70
	b	.LBB365_13
.LBB365_72:
	lsr	x12, x21, #2
	add	x12, x12, #1
	cmp	x21, #60
	b.hs	.LBB365_76
	mov	x26, xzr
	b	.LBB365_80
.LBB365_74:
	lsr	x27, x21, #1
	add	x20, x27, #1
	cmp	x21, #32
	str	x27, [sp, #16]
	b.hs	.LBB365_85
	mov	x26, xzr
	b	.LBB365_88
.LBB365_76:
	and	x21, x12, #0xc
	dup	v3.4s, w23
	lsl	x25, x10, #2
	and	x26, x12, #0x7ffffffffffffff0
	and	x23, x12, #0x7ffffffffffffff0
	add	x27, x25, x9, lsl #2
	add	x25, x11, x26, lsl #2
	add	x27, x24, x27
	add	x27, x27, #32
	mov	v4.16b, v3.16b
	mov	v5.16b, v3.16b
	mov	v6.16b, v3.16b
	.p2align	5, , 16
.LBB365_77:
	ldp	q7, q16, [x27]
	ldp	q17, q18, [x27, #-32]
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
	subs	x23, x23, #16
	add	x27, x27, #64
	b.ne	.LBB365_77
	umax	v3.4s, v3.4s, v4.4s
	umax	v3.4s, v3.4s, v5.4s
	umax	v3.4s, v3.4s, v6.4s
	umaxv	s3, v3.4s
	fmov	w23, s3
	cmp	x12, x26
	b.eq	.LBB365_13
	cbz	x21, .LBB365_83
.LBB365_80:
	and	x21, x12, #0x7ffffffffffffffc
	add	x25, x11, x21, lsl #2
	dup	v3.4s, w23
	sub	x23, x26, x21
	lsl	x10, x10, #2
	add	x10, x10, x26, lsl #2
	add	x9, x10, x9, lsl #2
	add	x9, x24, x9
	.p2align	5, , 16
.LBB365_81:
	ldr	q4, [x9], #16
	fmul	v4.4s, v4.4s, v2.s[0]
	fabs	v4.4s, v4.4s
	umax	v3.4s, v4.4s, v3.4s
	adds	x23, x23, #4
	b.ne	.LBB365_81
	umaxv	s3, v3.4s
	fmov	w23, s3
	cmp	x12, x21
	b.eq	.LBB365_13
.LBB365_83:
	add	x9, x11, x22, lsl #2
	.p2align	5, , 16
.LBB365_84:
	ldr	s3, [x25], #4
	fmul	s3, s2, s3
	fmov	w10, s3
	and	w10, w10, #0x7fffffff
	cmp	w10, w23
	csel	w23, w10, w23, hi
	cmp	x25, x9
	b.ne	.LBB365_84
	b	.LBB365_13
.LBB365_85:
	ands	x21, x20, #0xf
	dup	v3.4s, w23
	mvn	x23, x27
	lsl	x27, x10, #2
	csel	x28, x28, x21, eq
	add	x21, x27, x9, lsl #2
	sub	x20, x20, x28
	stp	x28, x20, [sp]
	add	x23, x23, x28
	add	x21, x24, x21
	sub	x28, x30, x20
	add	x27, x26, x20, lsl #3
	add	x21, x21, #64
	lsl	x28, x28, #1
	mov	v4.16b, v3.16b
	mov	v5.16b, v3.16b
	mov	v6.16b, v3.16b
.LBB365_86:
	sub	x20, x21, #48
	ldur	q7, [x21, #-64]
	mov	v7.s[1], v7.s[2]
	ld1	{ v7.s }[2], [x20]
	sub	x20, x21, #40
	ld1	{ v7.s }[3], [x20]
	sub	x20, x21, #16
	ldur	q16, [x21, #-32]
	mov	v16.s[1], v16.s[2]
	ld1	{ v16.s }[2], [x20]
	sub	x20, x21, #8
	ld1	{ v16.s }[3], [x20]
	add	x20, x21, #16
	ldr	q17, [x21]
	mov	v17.s[1], v17.s[2]
	ld1	{ v17.s }[2], [x20]
	add	x20, x21, #24
	ld1	{ v17.s }[3], [x20]
	add	x20, x21, #48
	ldr	q18, [x21, #32]
	mov	v18.s[1], v18.s[2]
	ld1	{ v18.s }[2], [x20]
	add	x20, x21, #56
	ld1	{ v18.s }[3], [x20]
	fmul	v7.4s, v7.4s, v2.s[0]
	fmul	v16.4s, v16.4s, v2.s[0]
	fmul	v17.4s, v17.4s, v2.s[0]
	fmul	v18.4s, v18.4s, v2.s[0]
	fabs	v7.4s, v7.4s
	fabs	v16.4s, v16.4s
	fabs	v17.4s, v17.4s
	fabs	v18.4s, v18.4s
	umax	v3.4s, v7.4s, v3.4s
	umax	v4.4s, v16.4s, v4.4s
	umax	v5.4s, v17.4s, v5.4s
	umax	v6.4s, v18.4s, v6.4s
	adds	x23, x23, #16
	add	x21, x21, #128
	b.ne	.LBB365_86
	umax	v3.4s, v3.4s, v4.4s
	umax	v3.4s, v3.4s, v5.4s
	umax	v3.4s, v3.4s, v6.4s
	umaxv	s3, v3.4s
	fmov	w23, s3
	ldp	x20, x26, [sp]
	cmp	x20, #5
	b.lo	.LBB365_91
.LBB365_88:
	ldr	x21, [sp, #16]
	add	x27, x21, #1
	ands	x20, x27, #0x3
	dup	v3.4s, w23
	mvn	x21, x21
	lsl	x23, x10, #2
	mov	w10, #4
	csel	x10, x10, x20, eq
	add	x20, x21, x26
	sub	x21, x27, x10
	add	x10, x20, x10
	sub	x20, x30, x21
	ldr	x27, [sp, #24]
	add	x27, x27, x21, lsl #3
	lsl	x28, x20, #1
	add	x20, x23, x26, lsl #3
	add	x9, x20, x9, lsl #2
	add	x9, x24, x9
	.p2align	5, , 16
.LBB365_89:
	add	x20, x9, #16
	ldr	q4, [x9]
	mov	v4.s[1], v4.s[2]
	ld1	{ v4.s }[2], [x20]
	add	x20, x9, #24
	ld1	{ v4.s }[3], [x20]
	fmul	v4.4s, v4.4s, v2.s[0]
	fabs	v4.4s, v4.4s
	umax	v3.4s, v4.4s, v3.4s
	adds	x10, x10, #4
	add	x9, x9, #32
	b.ne	.LBB365_89
	umaxv	s3, v3.4s
	fmov	w23, s3
.LBB365_91:
	ldr	x26, [sp, #24]
	.p2align	5, , 16
.LBB365_92:
	ldr	s3, [x27], #8
	sub	x28, x28, #2
	fmul	s3, s2, s3
	fmov	w9, s3
	and	w9, w9, #0x7fffffff
	cmp	w9, w23
	csel	w23, w9, w23, hi
	cbnz	x28, .LBB365_92
.LBB365_93:
	cmp	x25, x22
	b.hs	.LBB365_96
	cmp	x12, x11
	mov	w28, #16
	b.hs	.LBB365_113
	ldr	s3, [x26, x12, lsl #2]
	fmul	s3, s2, s3
	fmov	w9, s3
	and	w9, w9, #0x7fffffff
	cmp	w9, w23
	csel	w23, w9, w23, hi
	b	.LBB365_13
.LBB365_96:
	mov	w28, #16
	b	.LBB365_13
.LBB365_97:
	mov	x9, xzr
.LBB365_98:
	ldr	x8, [x0, #80]
	add	x8, x8, x9
	str	x8, [x0, #80]
	mov	w8, #65280
	orr	x12, x8, x23, lsl #32
	b	.LBB365_104
.LBB365_99:
	mov	w12, #1281
	b	.LBB365_104
.LBB365_100:
	mov	w12, #4097
	b	.LBB365_104
.LBB365_101:
	cmp	x17, x2
	b.eq	.LBB365_5
.LBB365_102:
	mov	w12, #769
	b	.LBB365_104
.LBB365_103:
	mov	w12, #2305
.LBB365_104:
	mov	x0, x12
	.cfi_def_cfa wsp, 288
	ldp	x20, x19, [sp, #272]
	ldp	x22, x21, [sp, #256]
	ldp	x24, x23, [sp, #240]
	ldp	x26, x25, [sp, #224]
	ldp	x28, x27, [sp, #208]
	ldp	x29, x30, [sp, #192]
	add	sp, sp, #288
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
.LBB365_105:
	.cfi_restore_state
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.601
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.601
	mov	x0, xzr
	mov	x1, x8
	mov	x2, x9
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_106:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.215
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.215
	mov	x0, x10
	mov	x1, x12
	mov	x2, x9
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_107:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.114
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.114
	mov	x0, xzr
	mov	x1, x9
	mov	w2, #2
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_108:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.581
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.581
	mov	x0, xzr
	mov	x1, x8
	mov	w2, #2
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_109:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.133
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.133
	mov	x0, x9
	mov	x1, x12
	mov	x2, x11
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_110:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.135
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.135
	mov	x0, xzr
	mov	x1, x12
	mov	x2, x11
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_111:
	adrp	x2, .Lanon.8c8d056469d251b282076468100da263.132
	add	x2, x2, :lo12:.Lanon.8c8d056469d251b282076468100da263.132
	mov	x0, x9
	mov	x1, x11
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.LBB365_112:
	adrp	x2, .Lanon.8c8d056469d251b282076468100da263.136
	add	x2, x2, :lo12:.Lanon.8c8d056469d251b282076468100da263.136
	mov	x0, x9
	mov	x1, x11
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.LBB365_113:
	adrp	x2, .Lanon.8c8d056469d251b282076468100da263.134
	add	x2, x2, :lo12:.Lanon.8c8d056469d251b282076468100da263.134
	mov	x0, x12
	mov	x1, x11
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.Lfunc_end365:
	.size	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak, .Lfunc_end365-_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.cfi_endproc
