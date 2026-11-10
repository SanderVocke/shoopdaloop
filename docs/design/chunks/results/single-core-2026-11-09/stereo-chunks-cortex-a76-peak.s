
	.section	.text._RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,"ax",@progbits
	.globl	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.p2align	2
	.prefalign	4, .Lfunc_end365, nop
	.type	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,@function
_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak:
	.cfi_startproc
	sub	sp, sp, #160
	.cfi_def_cfa_offset 160
	stp	d11, d10, [sp, #32]
	stp	d9, d8, [sp, #48]
	stp	x29, x30, [sp, #64]
	stp	x28, x27, [sp, #80]
	stp	x26, x25, [sp, #96]
	stp	x24, x23, [sp, #112]
	stp	x22, x21, [sp, #128]
	stp	x20, x19, [sp, #144]
	add	x29, sp, #64
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
	.cfi_offset b8, -104
	.cfi_offset b9, -112
	.cfi_offset b10, -120
	.cfi_offset b11, -128
	.cfi_remember_state
	ldr	x8, [x0, #32]
	ldr	x9, [x0, #48]
	cmp	x9, x8
	b.hi	.LBB365_93
	ldr	x8, [x1, #80]
	cbz	x8, .LBB365_4
	subs	x14, x2, #1
	b.hi	.LBB365_6
	ldp	s2, s0, [x1, #88]
	movi	d1, #0000000000000000
	b	.LBB365_7
.LBB365_4:
	mov	x9, xzr
	mov	w11, #65280
.LBB365_5:
	ldr	x8, [x0, #80]
	add	x8, x8, x9
	str	x8, [x0, #80]
	b	.LBB365_87
.LBB365_6:
	ldp	s2, s0, [x1, #88]
	fsub	s1, s0, s2
	ucvtf	s3, x14
	fdiv	s1, s1, s3
.LBB365_7:
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
	csel	w15, wzr, w10, ne
	cmp	x8, #3
	b.hs	.LBB365_95
	mov	w24, wzr
	mov	x16, xzr
	ldr	x17, [x0, #24]
	mov	w10, #40
	ldr	x18, [x0, #40]
	ldr	w4, [x3, #1032]
	ldr	x5, [x3, #552]
	ldr	x3, [x3, #544]
	madd	x6, x8, x10, x1
	mov	w7, #24
	add	x19, x17, #32
	mov	w28, #16
	mov	x22, x1
	b	.LBB365_11
	.p2align	5, , 16
.LBB365_9:
	ucvtf	s3, x23
	fmul	s3, s1, s3
	fadd	s2, s2, s3
.LBB365_10:
	add	x22, x22, #40
	add	x16, x23, x16
	cmp	x22, x6
	b.eq	.LBB365_82
.LBB365_11:
	ldr	x23, [x22, #32]
	sub	x25, x2, x16
	cmp	x23, x25
	b.hi	.LBB365_85
	ldr	w10, [x22]
	cbz	w10, .LBB365_9
	cmp	w10, #1
	b.ne	.LBB365_21
	ldr	x10, [x22, #16]
	cmp	x10, x18
	b.ne	.LBB365_86
	ldr	x13, [x22, #8]
	adds	x10, x13, x23
	b.hs	.LBB365_86
	cmp	x10, x9
	mov	w11, #2305
	b.hi	.LBB365_87
	add	x10, x17, x13, lsl #2
	lsl	x11, x23, #2
	tbz	w15, #0, .LBB365_41
	cbz	x23, .LBB365_10
	sub	x11, x11, #4
	cmp	x11, #11
	b.hi	.LBB365_48
	mov	x11, x10
	b	.LBB365_60
	.p2align	5, , 16
.LBB365_21:
	ldr	w10, [x22, #4]
	cmp	w10, w4
	b.ne	.LBB365_80
	ldr	w10, [x22, #8]
	cmp	x5, x10
	b.ls	.LBB365_80
	ldr	x25, [x3, x10, lsl #3]
	cbz	x25, .LBB365_80
	ldr	w10, [x22, #12]
	ldr	w11, [x25, #216]
	cmp	w10, w11
	b.ne	.LBB365_80
	ldr	w10, [x22, #16]
	ldr	x11, [x25, #112]
	cmp	x11, x10
	b.ls	.LBB365_80
	ldr	w11, [x22, #20]
	ldr	x12, [x25, #104]
	umaddl	x10, w10, w7, x12
	ldr	w12, [x10, #16]
	cmp	w12, w11
	b.ne	.LBB365_80
	ldr	x11, [x10]
	cbz	x11, .LBB365_80
	ldr	x11, [x25, #144]
	cmn	x11, #1
	b.eq	.LBB365_86
	ldr	x12, [x10, #8]
	ldp	x11, x10, [x25, #176]
	mul	x13, x10, x12
	add	x12, x10, x13
	cmp	x12, x13
	b.lo	.LBB365_94
	cmp	x12, x11
	b.hi	.LBB365_94
	cbz	x23, .LBB365_10
	ldp	w12, w21, [x22, #24]
	sub	x11, x23, #1
	madd	x11, x11, x21, x12
	cmp	x11, x12
	b.lo	.LBB365_81
	cmp	x11, x10
	mov	w11, #4097
	b.hs	.LBB365_87
	ldr	x30, [x25, #168]
	add	x25, x30, x13, lsl #2
	tbz	w15, #0, .LBB365_50
	cmp	x23, #4
	b.lo	.LBB365_62
	cmp	w21, #2
	b.ne	.LBB365_62
	sub	x10, x10, x12
	lsr	x11, x23, #2
	lsr	x21, x10, #3
	cmp	x21, x11
	csel	x26, x21, x11, lo
	lsl	x11, x26, #3
	cmp	x11, x10
	b.hi	.LBB365_96
	add	x25, x25, x12, lsl #2
	cbz	x26, .LBB365_75
	sub	x21, x11, #8
	cmp	x21, #32
	b.hs	.LBB365_65
	mov	x28, x11
	mov	x27, x25
	b	.LBB365_73
.LBB365_41:
	cbz	x23, .LBB365_10
	mov	x21, xzr
	subs	x11, x11, #4
	b.eq	.LBB365_47
	lsr	x12, x11, #2
	add	x12, x12, #1
	and	x12, x12, #0x7ffffffffffffffe
	sub	x13, x25, #1
	sub	x25, x25, #2
	.p2align	5, , 16
.LBB365_44:
	cmp	x2, #2
	cset	w20, lo
	add	x26, x10, x21, lsl #2
	ldp	s3, s4, [x26]
	cmp	x13, x21
	csinc	w27, w20, wzr, eq
	cmp	w27, #0
	fcsel	s2, s2, s0, ne
	fmul	s3, s2, s3
	fmov	w26, s3
	and	w26, w26, #0x7fffffff
	cmp	w26, w24
	csel	w24, w26, w24, hi
	fadd	s2, s1, s2
	cmp	x25, x21
	add	x21, x21, #2
	csinc	w20, w20, wzr, eq
	cmp	w20, #0
	fcsel	s2, s2, s0, ne
	fmul	s3, s2, s4
	fmov	w20, s3
	and	w20, w20, #0x7fffffff
	cmp	w20, w24
	csel	w24, w20, w24, hi
	fadd	s2, s1, s2
	cmp	x12, x21
	b.ne	.LBB365_44
	tbnz	w11, #2, .LBB365_10
	add	x10, x10, x21, lsl #2
.LBB365_47:
	add	x11, x16, #1
	add	x11, x11, x21
	cmp	x11, x2
	ccmp	x2, #2, #0, eq
	ldr	s3, [x10]
	fcsel	s2, s2, s0, lo
	fmul	s3, s2, s3
	fmov	w10, s3
	and	w10, w10, #0x7fffffff
	cmp	w10, w24
	csel	w24, w10, w24, hi
	fadd	s2, s1, s2
	b	.LBB365_10
.LBB365_48:
	lsr	x12, x11, #2
	add	x12, x12, #1
	cmp	x11, #60
	b.hs	.LBB365_53
	mov	x21, xzr
	b	.LBB365_57
.LBB365_50:
	sub	x11, x14, x16
	mov	x13, x23
	.p2align	5, , 16
.LBB365_51:
	cmp	x12, x10
	b.hs	.LBB365_97
	ldr	s3, [x25, x12, lsl #2]
	cmp	x11, #0
	ccmp	x2, #1, #0, eq
	fcsel	s2, s0, s2, hi
	fmul	s3, s2, s3
	fmov	w26, s3
	and	w26, w26, #0x7fffffff
	fadd	s2, s1, s2
	cmp	w26, w24
	csel	w24, w26, w24, hi
	sub	x13, x13, #1
	sub	x11, x11, #1
	add	x12, x12, x21
	cbnz	x13, .LBB365_51
	b	.LBB365_10
.LBB365_53:
	and	x25, x12, #0xc
	and	x21, x12, #0x7ffffffffffffff0
	add	x11, x10, x21, lsl #2
	dup	v3.4s, w24
	add	x24, x19, x13, lsl #2
	and	x26, x12, #0x7ffffffffffffff0
	mov	v4.16b, v3.16b
	mov	v5.16b, v3.16b
	mov	v6.16b, v3.16b
	.p2align	5, , 16
.LBB365_54:
	ldp	q7, q16, [x24]
	ldp	q17, q18, [x24, #-32]
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
	subs	x26, x26, #16
	add	x24, x24, #64
	b.ne	.LBB365_54
	umax	v3.4s, v3.4s, v4.4s
	umax	v3.4s, v3.4s, v5.4s
	umax	v3.4s, v3.4s, v6.4s
	umaxv	s3, v3.4s
	fmov	w24, s3
	cmp	x12, x21
	b.eq	.LBB365_10
	cbz	x25, .LBB365_60
.LBB365_57:
	and	x25, x12, #0x7ffffffffffffffc
	add	x11, x10, x25, lsl #2
	dup	v3.4s, w24
	sub	x24, x21, x25
	lsl	x20, x21, #2
	add	x13, x20, x13, lsl #2
	add	x13, x17, x13
	.p2align	5, , 16
.LBB365_58:
	ldr	q4, [x13], #16
	fmul	v4.4s, v4.4s, v2.s[0]
	fabs	v4.4s, v4.4s
	umax	v3.4s, v4.4s, v3.4s
	adds	x24, x24, #4
	b.ne	.LBB365_58
	umaxv	s3, v3.4s
	fmov	w24, s3
	cmp	x12, x25
	b.eq	.LBB365_10
.LBB365_60:
	add	x10, x10, x23, lsl #2
	.p2align	5, , 16
.LBB365_61:
	ldr	s3, [x11], #4
	fmul	s3, s2, s3
	fmov	w12, s3
	and	w12, w12, #0x7fffffff
	cmp	w12, w24
	csel	w24, w12, w24, hi
	cmp	x11, x10
	b.ne	.LBB365_61
	b	.LBB365_10
.LBB365_62:
	mov	x11, x23
	.p2align	5, , 16
.LBB365_63:
	cmp	x12, x10
	b.hs	.LBB365_98
	ldr	s3, [x25, x12, lsl #2]
	fmul	s3, s2, s3
	fmov	w13, s3
	and	w13, w13, #0x7fffffff
	cmp	w13, w24
	csel	w24, w13, w24, hi
	subs	x11, x11, #1
	add	x12, x12, x21
	b.ne	.LBB365_63
	b	.LBB365_10
.LBB365_65:
	lsr	x27, x21, #3
	add	x20, x27, #1
	cmp	x21, #128
	str	x27, [sp, #24]
	b.hs	.LBB365_67
	mov	x19, xzr
	b	.LBB365_70
.LBB365_67:
	ands	x21, x20, #0xf
	dup	v3.4s, w24
	mvn	x24, x27
	lsl	x27, x13, #2
	csel	x28, x28, x21, eq
	add	x21, x27, x12, lsl #2
	sub	x19, x20, x28
	stp	x28, x19, [sp, #8]
	add	x24, x24, x28
	add	x21, x30, x21
	sub	x28, x26, x19
	add	x27, x25, x19, lsl #5
	add	x21, x21, #256
	lsl	x28, x28, #3
	mov	v4.16b, v3.16b
	mov	v5.16b, v3.16b
	mov	v6.16b, v3.16b
	.p2align	5, , 16
.LBB365_68:
	ldp	q21, q16, [x21, #-176]
	ldp	q17, q20, [x21, #-224]
	ldp	q19, q18, [x21, #-256]
	zip1	v7.4s, v19.4s, v17.4s
	ldur	q22, [x21, #-192]
	zip1	v23.4s, v22.4s, v16.4s
	trn1	v24.4s, v22.4s, v16.4s
	zip2	v16.4s, v19.4s, v17.4s
	ldur	d19, [x21, #-144]
	zip1	v17.4s, v18.4s, v20.4s
	zip1	v25.4s, v21.4s, v19.4s
	dup	v18.4s, v18.s[2]
	ldp	q27, q26, [x21, #-96]
	ldp	q29, q28, [x21, #-128]
	zip1	v19.4s, v29.4s, v27.4s
	ext	v22.16b, v22.16b, v23.16b, #8
	mov	v16.d[1], v24.d[1]
	ext	v23.16b, v21.16b, v25.16b, #8
	mov	v18.s[1], v20.s[2]
	ldp	q24, q20, [x21, #-48]
	ldur	q25, [x21, #-64]
	zip1	v30.4s, v25.4s, v20.4s
	mov	v7.d[1], v22.d[1]
	mov	v17.d[1], v23.d[1]
	ext	v22.16b, v25.16b, v30.16b, #8
	mov	v18.s[2], v21.s[2]
	mov	v19.d[1], v22.d[1]
	sub	x20, x21, #136
	ld1	{ v18.s }[3], [x20]
	trn1	v22.4s, v25.4s, v20.4s
	dup	v20.4s, v28.s[2]
	mov	v20.s[1], v26.s[2]
	mov	v20.s[2], v24.s[2]
	sub	x20, x21, #8
	ld1	{ v20.s }[3], [x20]
	zip2	v21.4s, v29.4s, v27.4s
	mov	v21.d[1], v22.d[1]
	ldp	q27, q29, [x21, #48]
	zip1	v22.4s, v28.4s, v26.4s
	ldp	q26, q30, [x21, #16]
	ldur	d23, [x21, #-16]
	ldp	q25, q28, [x21, #80]
	zip1	v23.4s, v24.4s, v23.4s
	ext	v23.16b, v24.16b, v23.16b, #8
	mov	v22.d[1], v23.d[1]
	ldr	q31, [x21]
	zip1	v23.4s, v31.4s, v30.4s
	zip1	v24.4s, v29.4s, v28.4s
	ext	v8.16b, v29.16b, v24.16b, #8
	dup	v24.4s, v26.s[2]
	mov	v24.s[1], v27.s[2]
	mov	v24.s[2], v25.s[2]
	add	x20, x21, #120
	ld1	{ v24.s }[3], [x20]
	mov	v23.d[1], v8.d[1]
	ldr	d8, [x21, #112]
	trn1	v28.4s, v29.4s, v28.4s
	zip2	v30.4s, v31.4s, v30.4s
	ldp	q31, q9, [x21, #176]
	mov	v30.d[1], v28.d[1]
	zip1	v26.4s, v26.4s, v27.4s
	ldp	q29, q27, [x21, #208]
	zip1	v8.4s, v25.4s, v8.4s
	ext	v25.16b, v25.16b, v8.16b, #8
	ldp	q28, q8, [x21, #144]
	mov	v26.d[1], v25.d[1]
	ldr	q25, [x21, #128]
	zip1	v10.4s, v25.4s, v8.4s
	zip1	v11.4s, v9.4s, v27.4s
	ext	v11.16b, v9.16b, v11.16b, #8
	mov	v10.d[1], v11.d[1]
	trn1	v27.4s, v9.4s, v27.4s
	zip2	v25.4s, v25.4s, v8.4s
	mov	v25.d[1], v27.d[1]
	ldr	d27, [x21, #240]
	zip1	v8.4s, v28.4s, v31.4s
	zip1	v27.4s, v29.4s, v27.4s
	ext	v27.16b, v29.16b, v27.16b, #8
	dup	v28.4s, v28.s[2]
	mov	v28.s[1], v31.s[2]
	mov	v28.s[2], v29.s[2]
	add	x20, x21, #248
	ld1	{ v28.s }[3], [x20]
	mov	v8.d[1], v27.d[1]
	fmul	v7.4s, v7.4s, v2.s[0]
	fmul	v19.4s, v19.4s, v2.s[0]
	fmul	v23.4s, v23.4s, v2.s[0]
	fmul	v27.4s, v10.4s, v2.s[0]
	fmul	v16.4s, v16.4s, v2.s[0]
	fmul	v21.4s, v21.4s, v2.s[0]
	fmul	v29.4s, v30.4s, v2.s[0]
	fmul	v25.4s, v25.4s, v2.s[0]
	fmul	v17.4s, v17.4s, v2.s[0]
	fmul	v22.4s, v22.4s, v2.s[0]
	fmul	v26.4s, v26.4s, v2.s[0]
	fmul	v30.4s, v8.4s, v2.s[0]
	fmul	v18.4s, v18.4s, v2.s[0]
	fmul	v20.4s, v20.4s, v2.s[0]
	fmul	v24.4s, v24.4s, v2.s[0]
	fmul	v28.4s, v28.4s, v2.s[0]
	fabs	v7.4s, v7.4s
	fabs	v19.4s, v19.4s
	fabs	v23.4s, v23.4s
	fabs	v27.4s, v27.4s
	umax	v3.4s, v7.4s, v3.4s
	umax	v4.4s, v19.4s, v4.4s
	umax	v5.4s, v23.4s, v5.4s
	umax	v6.4s, v27.4s, v6.4s
	fabs	v7.4s, v16.4s
	fabs	v16.4s, v21.4s
	fabs	v19.4s, v29.4s
	fabs	v21.4s, v25.4s
	umax	v3.4s, v7.4s, v3.4s
	umax	v4.4s, v16.4s, v4.4s
	umax	v5.4s, v19.4s, v5.4s
	umax	v6.4s, v21.4s, v6.4s
	fabs	v7.4s, v17.4s
	fabs	v16.4s, v22.4s
	fabs	v17.4s, v26.4s
	fabs	v19.4s, v30.4s
	umax	v3.4s, v7.4s, v3.4s
	umax	v4.4s, v16.4s, v4.4s
	umax	v5.4s, v17.4s, v5.4s
	umax	v6.4s, v19.4s, v6.4s
	fabs	v7.4s, v18.4s
	fabs	v16.4s, v20.4s
	fabs	v17.4s, v24.4s
	fabs	v18.4s, v28.4s
	umax	v3.4s, v7.4s, v3.4s
	umax	v4.4s, v16.4s, v4.4s
	umax	v5.4s, v17.4s, v5.4s
	umax	v6.4s, v18.4s, v6.4s
	adds	x24, x24, #16
	add	x21, x21, #512
	b.ne	.LBB365_68
	umax	v3.4s, v3.4s, v4.4s
	umax	v3.4s, v3.4s, v5.4s
	umax	v3.4s, v3.4s, v6.4s
	umaxv	s3, v3.4s
	fmov	w24, s3
	ldr	x19, [sp, #8]
	cmp	x19, #5
	ldr	x19, [sp, #16]
	b.lo	.LBB365_73
.LBB365_70:
	ldr	x21, [sp, #24]
	add	x28, x21, #1
	ands	x20, x28, #0x3
	dup	v3.4s, w24
	mvn	x21, x21
	lsl	x24, x19, #5
	mov	w27, #4
	csel	x20, x27, x20, eq
	add	x21, x21, x19
	sub	x27, x28, x20
	add	x21, x21, x20
	sub	x20, x26, x27
	add	x27, x25, x27, lsl #5
	lsl	x28, x20, #3
	add	x13, x24, x13, lsl #2
	add	x12, x13, x12, lsl #2
	add	x12, x30, x12
	.p2align	5, , 16
.LBB365_71:
	ldr	d4, [x12, #112]
	ldp	q5, q18, [x12, #16]
	ldp	q7, q6, [x12, #80]
	ldp	q17, q16, [x12, #48]
	add	x13, x12, #120
	ldr	q19, [x12], #128
	zip1	v20.4s, v19.4s, v18.4s
	zip1	v21.4s, v16.4s, v6.4s
	ext	v21.16b, v16.16b, v21.16b, #8
	mov	v20.d[1], v21.d[1]
	trn1	v6.4s, v16.4s, v6.4s
	zip2	v16.4s, v19.4s, v18.4s
	mov	v16.d[1], v6.d[1]
	zip1	v6.4s, v5.4s, v17.4s
	zip1	v4.4s, v7.4s, v4.4s
	ext	v4.16b, v7.16b, v4.16b, #8
	dup	v5.4s, v5.s[2]
	mov	v5.s[1], v17.s[2]
	mov	v5.s[2], v7.s[2]
	ld1	{ v5.s }[3], [x13]
	mov	v6.d[1], v4.d[1]
	fmul	v4.4s, v20.4s, v2.s[0]
	fmul	v7.4s, v16.4s, v2.s[0]
	fmul	v6.4s, v6.4s, v2.s[0]
	fmul	v5.4s, v5.4s, v2.s[0]
	fabs	v4.4s, v4.4s
	umax	v3.4s, v4.4s, v3.4s
	fabs	v4.4s, v7.4s
	umax	v3.4s, v4.4s, v3.4s
	fabs	v4.4s, v6.4s
	umax	v3.4s, v4.4s, v3.4s
	fabs	v4.4s, v5.4s
	umax	v3.4s, v4.4s, v3.4s
	adds	x21, x21, #4
	b.ne	.LBB365_71
	umaxv	s3, v3.4s
	fmov	w24, s3
.LBB365_73:
	add	x19, x17, #32
	.p2align	5, , 16
.LBB365_74:
	ldr	s3, [x27]
	add	x12, x27, #8
	ld1	{ v3.s }[1], [x12]
	add	x12, x27, #16
	ld1	{ v3.s }[2], [x12]
	add	x12, x27, #24
	ld1	{ v3.s }[3], [x12]
	fmul	v3.4s, v3.4s, v2.s[0]
	fabs	v3.4s, v3.4s
	umaxv	s3, v3.4s
	fmov	w12, s3
	cmp	w12, w24
	csel	w24, w12, w24, hi
	add	x27, x27, #32
	subs	x28, x28, #8
	b.ne	.LBB365_74
.LBB365_75:
	cmp	x23, x26, lsl #2
	b.ls	.LBB365_79
	lsl	x12, x26, #2
	sub	x12, x23, x12
	mov	w28, #16
	.p2align	5, , 16
.LBB365_77:
	cmp	x11, x10
	b.hs	.LBB365_99
	ldr	s3, [x25, x11, lsl #2]
	fmul	s3, s2, s3
	fmov	w13, s3
	and	w13, w13, #0x7fffffff
	cmp	w13, w24
	csel	w24, w13, w24, hi
	subs	x12, x12, #1
	add	x11, x11, #2
	b.ne	.LBB365_77
	b	.LBB365_10
.LBB365_79:
	mov	w28, #16
	b	.LBB365_10
.LBB365_80:
	mov	w11, #1281
	b	.LBB365_87
.LBB365_81:
	mov	w11, #4097
	b	.LBB365_87
.LBB365_82:
	cmp	x16, x2
	b.ne	.LBB365_85
	cmp	x8, #1
	b.ne	.LBB365_88
	mov	x10, xzr
	mov	x9, xzr
	b	.LBB365_91
.LBB365_85:
	mov	w11, #769
	b	.LBB365_87
.LBB365_86:
	mov	w11, #2305
.LBB365_87:
	mov	x0, x11
	.cfi_def_cfa wsp, 160
	ldp	x20, x19, [sp, #144]
	ldp	x22, x21, [sp, #128]
	ldp	x24, x23, [sp, #112]
	ldp	x26, x25, [sp, #96]
	ldp	x28, x27, [sp, #80]
	ldp	x29, x30, [sp, #64]
	ldp	d9, d8, [sp, #48]
	ldp	d11, d10, [sp, #32]
	add	sp, sp, #160
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
	.cfi_restore b8
	.cfi_restore b9
	.cfi_restore b10
	.cfi_restore b11
	ret
.LBB365_88:
	.cfi_restore_state
	mov	x10, xzr
	mov	x9, xzr
	and	x11, x8, #0x2
	add	x12, x1, #40
	.p2align	5, , 16
.LBB365_89:
	ldur	w13, [x12, #-40]
	ldur	x14, [x12, #-8]
	ldr	w15, [x12]
	ldr	x16, [x12, #32]
	cmp	w13, #0
	csel	x13, xzr, x14, eq
	cmp	w15, #0
	add	x9, x13, x9
	csel	x13, xzr, x16, eq
	add	x9, x13, x9
	add	x10, x10, #2
	add	x12, x12, #80
	cmp	x11, x10
	b.ne	.LBB365_89
	tbz	w8, #0, .LBB365_92
.LBB365_91:
	mov	w8, #40
	madd	x8, x10, x8, x1
	ldr	w10, [x8]
	ldr	x8, [x8, #32]
	cmp	w10, #0
	csel	x8, xzr, x8, eq
	add	x9, x8, x9
.LBB365_92:
	mov	w8, #65280
	orr	x11, x8, x24, lsl #32
	b	.LBB365_5
.LBB365_93:
	adrp	x3, .Lanon.030b3f8a341b026392beaa917442ffb0.599
	add	x3, x3, :lo12:.Lanon.030b3f8a341b026392beaa917442ffb0.599
	mov	x0, xzr
	mov	x1, x9
	mov	x2, x8
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_94:
	adrp	x3, .Lanon.030b3f8a341b026392beaa917442ffb0.214
	add	x3, x3, :lo12:.Lanon.030b3f8a341b026392beaa917442ffb0.214
	mov	x0, x13
	mov	x1, x12
	mov	x2, x11
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_95:
	adrp	x3, .Lanon.030b3f8a341b026392beaa917442ffb0.114
	add	x3, x3, :lo12:.Lanon.030b3f8a341b026392beaa917442ffb0.114
	mov	x0, xzr
	mov	x1, x8
	mov	w2, #2
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_96:
	adrp	x3, .Lanon.030b3f8a341b026392beaa917442ffb0.134
	add	x3, x3, :lo12:.Lanon.030b3f8a341b026392beaa917442ffb0.134
	mov	x0, xzr
	mov	x1, x11
	mov	x2, x10
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_97:
	adrp	x2, .Lanon.030b3f8a341b026392beaa917442ffb0.132
	add	x2, x2, :lo12:.Lanon.030b3f8a341b026392beaa917442ffb0.132
	mov	x0, x12
	mov	x1, x10
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.LBB365_98:
	adrp	x2, .Lanon.030b3f8a341b026392beaa917442ffb0.135
	add	x2, x2, :lo12:.Lanon.030b3f8a341b026392beaa917442ffb0.135
	mov	x0, x12
	mov	x1, x10
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.LBB365_99:
	adrp	x2, .Lanon.030b3f8a341b026392beaa917442ffb0.133
	add	x2, x2, :lo12:.Lanon.030b3f8a341b026392beaa917442ffb0.133
	mov	x0, x11
	mov	x1, x10
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.Lfunc_end365:
	.size	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak, .Lfunc_end365-_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.cfi_endproc
