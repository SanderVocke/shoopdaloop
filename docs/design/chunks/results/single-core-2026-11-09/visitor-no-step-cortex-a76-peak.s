
	.section	.text._RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,"ax",@progbits
	.globl	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.p2align	2
	.prefalign	4, .Lfunc_end365, nop
	.type	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak,@function
_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak:
	.cfi_startproc
	stp	x29, x30, [sp, #-96]!
	.cfi_def_cfa_offset 96
	str	x27, [sp, #16]
	stp	x26, x25, [sp, #32]
	stp	x24, x23, [sp, #48]
	stp	x22, x21, [sp, #64]
	stp	x20, x19, [sp, #80]
	mov	x29, sp
	.cfi_def_cfa w29, 96
	.cfi_offset w19, -8
	.cfi_offset w20, -16
	.cfi_offset w21, -24
	.cfi_offset w22, -32
	.cfi_offset w23, -40
	.cfi_offset w24, -48
	.cfi_offset w25, -56
	.cfi_offset w26, -64
	.cfi_offset w27, -80
	.cfi_offset w30, -88
	.cfi_offset w29, -96
	.cfi_remember_state
	ldr	x8, [x0, #32]
	ldr	x9, [x0, #48]
	cmp	x9, x8
	b.hi	.LBB365_71
	ldr	x8, [x1, #128]
	cbz	x8, .LBB365_56
	ldp	s2, s0, [x1, #136]
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
	csel	w14, wzr, w10, ne
	cmp	x2, #2
	csinc	w10, w14, wzr, hs
	movi	d1, #0000000000000000
	tbnz	w10, #0, .LBB365_4
	fsub	s1, s0, s2
	sub	x10, x2, #1
	ucvtf	s3, x10
	fdiv	s1, s1, s3
.LBB365_4:
	cmp	x8, #3
	b.hs	.LBB365_73
	mov	w19, wzr
	mov	x15, xzr
	ldr	x16, [x0, #24]
	ldr	x17, [x0, #40]
	add	x18, x1, x8, lsl #6
	ldr	w4, [x3, #1032]
	ldr	x5, [x3, #552]
	ldr	x3, [x3, #544]
	sub	x6, x2, #1
	add	x7, x16, #32
	mov	w20, #24
	mov	x21, x1
	b	.LBB365_8
	.p2align	5, , 16
.LBB365_6:
	ucvtf	s3, x22
	fmul	s3, s1, s3
	fadd	s2, s2, s3
.LBB365_7:
	add	x21, x21, #64
	add	x15, x22, x15
	cmp	x21, x18
	b.eq	.LBB365_60
.LBB365_8:
	ldr	x22, [x21, #56]
	sub	x24, x2, x15
	cmp	x22, x24
	b.hi	.LBB365_63
	ldr	x10, [x21]
	cbz	x10, .LBB365_6
	cmp	x10, #1
	b.ne	.LBB365_18
	ldr	x10, [x21, #16]
	cmp	x10, x17
	b.ne	.LBB365_64
	ldr	x13, [x21, #8]
	adds	x10, x13, x22
	b.hs	.LBB365_64
	cmp	x10, x9
	mov	w12, #2305
	b.hi	.LBB365_65
	add	x10, x16, x13, lsl #2
	lsl	x11, x22, #2
	tbz	w14, #0, .LBB365_35
	cbz	x22, .LBB365_7
	sub	x11, x11, #4
	cmp	x11, #11
	b.hi	.LBB365_42
	mov	x11, x10
	b	.LBB365_54
	.p2align	5, , 16
.LBB365_18:
	ldr	w10, [x21, #16]
	cmp	w10, w4
	b.ne	.LBB365_58
	ldr	x10, [x21, #8]
	cmp	x10, x5
	mov	w12, #1281
	b.hs	.LBB365_65
	ldr	x24, [x3, x10, lsl #3]
	cbz	x24, .LBB365_58
	ldr	w10, [x21, #40]
	ldr	w11, [x24, #216]
	cmp	w10, w11
	b.ne	.LBB365_58
	ldr	w10, [x21, #44]
	ldr	x11, [x24, #112]
	cmp	x11, x10
	b.ls	.LBB365_58
	ldr	w11, [x21, #48]
	ldr	x12, [x24, #104]
	umaddl	x10, w10, w20, x12
	ldr	w12, [x10, #16]
	cmp	w12, w11
	b.ne	.LBB365_58
	ldr	x11, [x10]
	cbz	x11, .LBB365_58
	ldr	x11, [x24, #144]
	cmn	x11, #1
	b.eq	.LBB365_64
	ldr	x12, [x10, #8]
	ldp	x11, x10, [x24, #176]
	mul	x13, x10, x12
	add	x12, x10, x13
	cmp	x12, x13
	b.lo	.LBB365_72
	cmp	x12, x11
	b.hi	.LBB365_72
	cbz	x22, .LBB365_7
	ldp	x11, x23, [x21, #24]
	sub	x12, x22, #1
	madd	x12, x23, x12, x11
	cmp	x12, x11
	b.lo	.LBB365_59
	cmp	x12, x10
	mov	w12, #4097
	b.hs	.LBB365_65
	ldr	x12, [x24, #168]
	add	x12, x12, x13, lsl #2
	tbz	w14, #0, .LBB365_44
	mov	x13, x22
	.p2align	5, , 16
.LBB365_33:
	cmp	x11, x10
	b.hs	.LBB365_75
	ldr	s3, [x12, x11, lsl #2]
	fmul	s3, s2, s3
	fmov	w24, s3
	and	w24, w24, #0x7fffffff
	cmp	w24, w19
	csel	w19, w24, w19, hi
	subs	x13, x13, #1
	add	x11, x11, x23
	b.ne	.LBB365_33
	b	.LBB365_7
.LBB365_35:
	cbz	x22, .LBB365_7
	mov	x23, xzr
	subs	x11, x11, #4
	b.eq	.LBB365_41
	lsr	x12, x11, #2
	add	x12, x12, #1
	and	x12, x12, #0x7ffffffffffffffe
	sub	x13, x24, #1
	sub	x24, x24, #2
	.p2align	5, , 16
.LBB365_38:
	cmp	x2, #1
	cset	w25, hi
	add	x26, x10, x23, lsl #2
	ldp	s3, s4, [x26]
	cmp	x13, x23
	cset	w27, eq
	tst	w25, w27
	fcsel	s2, s0, s2, ne
	fmul	s3, s2, s3
	fmov	w26, s3
	and	w26, w26, #0x7fffffff
	cmp	w26, w19
	csel	w19, w26, w19, hi
	fadd	s2, s1, s2
	cmp	x24, x23
	add	x23, x23, #2
	cset	w26, eq
	tst	w25, w26
	fcsel	s2, s0, s2, ne
	fmul	s3, s2, s4
	fmov	w25, s3
	and	w25, w25, #0x7fffffff
	cmp	w25, w19
	csel	w19, w25, w19, hi
	fadd	s2, s1, s2
	cmp	x12, x23
	b.ne	.LBB365_38
	tbnz	w11, #2, .LBB365_7
	add	x10, x10, x23, lsl #2
.LBB365_41:
	add	x11, x15, #1
	add	x11, x11, x23
	cmp	x11, x2
	ccmp	x2, #1, #0, eq
	ldr	s3, [x10]
	fcsel	s2, s0, s2, hi
	fmul	s3, s2, s3
	fmov	w10, s3
	and	w10, w10, #0x7fffffff
	cmp	w10, w19
	csel	w19, w10, w19, hi
	fadd	s2, s1, s2
	b	.LBB365_7
.LBB365_42:
	lsr	x12, x11, #2
	add	x12, x12, #1
	cmp	x11, #60
	b.hs	.LBB365_47
	mov	x23, xzr
	b	.LBB365_51
.LBB365_44:
	sub	x13, x6, x15
	mov	x24, x22
	.p2align	5, , 16
.LBB365_45:
	cmp	x11, x10
	b.hs	.LBB365_74
	ldr	s3, [x12, x11, lsl #2]
	cmp	x13, #0
	ccmp	x2, #1, #0, eq
	fcsel	s2, s0, s2, hi
	fmul	s3, s2, s3
	fmov	w25, s3
	and	w25, w25, #0x7fffffff
	fadd	s2, s1, s2
	cmp	w25, w19
	csel	w19, w25, w19, hi
	sub	x24, x24, #1
	sub	x13, x13, #1
	add	x11, x11, x23
	cbnz	x24, .LBB365_45
	b	.LBB365_7
.LBB365_47:
	and	x24, x12, #0xc
	and	x23, x12, #0x7ffffffffffffff0
	add	x11, x10, x23, lsl #2
	dup	v3.4s, w19
	add	x19, x7, x13, lsl #2
	and	x25, x12, #0x7ffffffffffffff0
	mov	v4.16b, v3.16b
	mov	v5.16b, v3.16b
	mov	v6.16b, v3.16b
	.p2align	5, , 16
.LBB365_48:
	ldp	q7, q16, [x19]
	ldp	q17, q18, [x19, #-32]
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
	add	x19, x19, #64
	b.ne	.LBB365_48
	umax	v3.4s, v3.4s, v4.4s
	umax	v3.4s, v3.4s, v5.4s
	umax	v3.4s, v3.4s, v6.4s
	umaxv	s3, v3.4s
	fmov	w19, s3
	cmp	x12, x23
	b.eq	.LBB365_7
	cbz	x24, .LBB365_54
.LBB365_51:
	and	x24, x12, #0x7ffffffffffffffc
	add	x11, x10, x24, lsl #2
	dup	v3.4s, w19
	sub	x19, x23, x24
	lsl	x23, x23, #2
	add	x13, x23, x13, lsl #2
	add	x13, x16, x13
	.p2align	5, , 16
.LBB365_52:
	ldr	q4, [x13], #16
	fmul	v4.4s, v4.4s, v2.s[0]
	fabs	v4.4s, v4.4s
	umax	v3.4s, v4.4s, v3.4s
	adds	x19, x19, #4
	b.ne	.LBB365_52
	umaxv	s3, v3.4s
	fmov	w19, s3
	cmp	x12, x24
	b.eq	.LBB365_7
.LBB365_54:
	add	x10, x10, x22, lsl #2
	.p2align	5, , 16
.LBB365_55:
	ldr	s3, [x11], #4
	fmul	s3, s2, s3
	fmov	w12, s3
	and	w12, w12, #0x7fffffff
	cmp	w12, w19
	csel	w19, w12, w19, hi
	cmp	x11, x10
	b.ne	.LBB365_55
	b	.LBB365_7
.LBB365_56:
	mov	x9, xzr
	mov	w12, #65280
.LBB365_57:
	ldr	x8, [x0, #80]
	add	x8, x8, x9
	str	x8, [x0, #80]
	b	.LBB365_65
.LBB365_58:
	mov	w12, #1281
	b	.LBB365_65
.LBB365_59:
	mov	w12, #4097
	b	.LBB365_65
.LBB365_60:
	cmp	x15, x2
	b.ne	.LBB365_63
	cmp	x8, #1
	b.ne	.LBB365_66
	mov	x10, xzr
	mov	x9, xzr
	b	.LBB365_69
.LBB365_63:
	mov	w12, #769
	b	.LBB365_65
.LBB365_64:
	mov	w12, #2305
.LBB365_65:
	mov	x0, x12
	.cfi_def_cfa wsp, 96
	ldp	x20, x19, [sp, #80]
	ldp	x22, x21, [sp, #64]
	ldp	x24, x23, [sp, #48]
	ldp	x26, x25, [sp, #32]
	ldr	x27, [sp, #16]
	ldp	x29, x30, [sp], #96
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
	.cfi_restore w30
	.cfi_restore w29
	ret
.LBB365_66:
	.cfi_restore_state
	mov	x10, xzr
	mov	x9, xzr
	and	x11, x8, #0x2
	add	x12, x1, #64
	.p2align	5, , 16
.LBB365_67:
	ldur	x13, [x12, #-64]
	ldp	x14, x15, [x12, #-8]
	ldr	x16, [x12, #56]
	cmp	x13, #0
	csel	x13, xzr, x14, eq
	cmp	x15, #0
	add	x9, x13, x9
	csel	x13, xzr, x16, eq
	add	x9, x13, x9
	add	x10, x10, #2
	add	x12, x12, #128
	cmp	x11, x10
	b.ne	.LBB365_67
	tbz	w8, #0, .LBB365_70
.LBB365_69:
	add	x8, x1, x10, lsl #6
	ldr	x10, [x8]
	ldr	x8, [x8, #56]
	cmp	x10, #0
	csel	x8, xzr, x8, eq
	add	x9, x8, x9
.LBB365_70:
	mov	w8, #65280
	orr	x12, x8, x19, lsl #32
	b	.LBB365_57
.LBB365_71:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.597
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.597
	mov	x0, xzr
	mov	x1, x9
	mov	x2, x8
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_72:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.212
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.212
	mov	x0, x13
	mov	x1, x12
	mov	x2, x11
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_73:
	adrp	x3, .Lanon.8c8d056469d251b282076468100da263.114
	add	x3, x3, :lo12:.Lanon.8c8d056469d251b282076468100da263.114
	mov	x0, xzr
	mov	x1, x8
	mov	w2, #2
	bl	_RNvNtNtCsaWcbhsS8OaW_4core5slice5index16slice_index_fail
.LBB365_74:
	adrp	x2, .Lanon.8c8d056469d251b282076468100da263.132
	add	x2, x2, :lo12:.Lanon.8c8d056469d251b282076468100da263.132
	mov	x0, x11
	mov	x1, x10
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.LBB365_75:
	adrp	x2, .Lanon.8c8d056469d251b282076468100da263.133
	add	x2, x2, :lo12:.Lanon.8c8d056469d251b282076468100da263.133
	mov	x0, x11
	mov	x1, x10
	bl	_RNvNtCsaWcbhsS8OaW_4core9panicking18panic_bounds_check
.Lfunc_end365:
	.size	_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak, .Lfunc_end365-_RNvMs2_NtCsu7vi79m8Ce_10rt_runtime10audio_refsNtB5_11RenderArena4peak
	.cfi_endproc
