
work/bin/arm-quantum-mask:	file format elf64-littleaarch64

Disassembly of section .text:

0000000000325e20 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick>:
  325e20:      	sub	sp, sp, #0x140
  325e24:      	str	d8, [sp, #0xd0]
  325e28:      	stp	x29, x30, [sp, #0xe0]
  325e2c:      	stp	x28, x27, [sp, #0xf0]
  325e30:      	stp	x26, x25, [sp, #0x100]
  325e34:      	stp	x24, x23, [sp, #0x110]
  325e38:      	stp	x22, x21, [sp, #0x120]
  325e3c:      	stp	x20, x19, [sp, #0x130]
  325e40:      	add	x29, sp, #0xe0
  325e44:      	cbnz	x2, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  325e48:      	ldrb	w9, [x0, #0x435]
  325e4c:      	tbnz	w9, #0x0, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  325e50:      	ldrb	w9, [x0, #0x436]
  325e54:      	tbnz	w9, #0x0, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  325e58:      	ldr	x9, [x0, #0x28]
  325e5c:      	cmp	x9, #0x2
  325e60:      	b.ne	0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  325e64:      	ldrb	w9, [x0, #0x434]
  325e68:      	tbnz	w9, #0x0, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  325e6c:      	ldrb	w9, [x0, #0x432]
  325e70:      	cbz	w9, 0x325ef8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0xd8>
  325e74:      	ldp	x20, x10, [x0, #0x140]
  325e78:      	lsl	x21, x10, #5
  325e7c:      	cbz	x10, 0x325ebc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  325e80:      	sub	x11, x21, #0x20
  325e84:      	lsr	w9, w11, #5
  325e88:      	add	w9, w9, #0x1
  325e8c:      	ands	x12, x9, #0x3
  325e90:      	mov	x9, x20
  325e94:      	b.eq	0x325eb4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x94>
  325e98:      	lsl	x12, x12, #5
  325e9c:      	mov	x9, x20
  325ea0:      	ldr	w13, [x9, #0x10]
  325ea4:      	cbnz	w13, 0x3263f4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5d4>
  325ea8:      	add	x9, x9, #0x20
  325eac:      	subs	x12, x12, #0x20
  325eb0:      	b.ne	0x325ea0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x80>
  325eb4:      	cmp	x11, #0x60
  325eb8:      	b.hs	0x3263c0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5a0>
  325ebc:      	ldr	x9, [x0, #0x420]
  325ec0:      	ldrh	w28, [x0, #0x430]
  325ec4:      	adds	x11, x9, x28
  325ec8:      	b.hs	0x3264d4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6b4>
  325ecc:      	str	xzr, [x0, #0x3d8]
  325ed0:      	movi	v0.2d, #0000000000000000
  325ed4:      	str	q0, [x0, #0x3f0]
  325ed8:      	cbz	x10, 0x3264a0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x680>
  325edc:      	add	x9, x0, #0x3e0
  325ee0:      	stp	x11, x9, [sp, #0x18]
  325ee4:      	sub	x22, x28, #0x1
  325ee8:      	mov	w13, #0x88              // =136
  325eec:      	mov	w14, #0x98              // =152
  325ef0:      	stp	x28, x22, [sp, #0x30]
  325ef4:      	b	0x325fb0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x190>
  325ef8:      	ldr	x9, [x0, #0x160]
  325efc:      	cbnz	x9, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  325f00:      	ldr	x10, [x0, #0x258]
  325f04:      	cbz	x10, 0x32634c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x52c>
  325f08:      	ldr	x9, [x0, #0x250]
  325f0c:      	lsl	x10, x10, #7
  325f10:      	b	0x325f2c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x10c>
  325f14:      	nop
  325f18:      	nop
  325f1c:      	nop
  325f20:      	add	x9, x9, #0x80
  325f24:      	subs	x10, x10, #0x80
  325f28:      	b.eq	0x32634c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x52c>
  325f2c:      	ldrb	w11, [x9, #0x78]
  325f30:      	cmp	w11, #0x2
  325f34:      	b.eq	0x325f20 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x100>
  325f38:      	ldrb	w11, [x9]
  325f3c:      	cmp	w11, #0xff
  325f40:      	b.ne	0x325f20 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x100>
  325f44:      	b	0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  325f48:      	ldp	q0, q1, [x29, #-0x40]
  325f4c:      	ldr	x9, [x0, #0x3d0]
  325f50:      	madd	x9, x27, x14, x9
  325f54:      	stur	q0, [x9, #0x18]
  325f58:      	stur	q1, [x9, #0x28]
  325f5c:      	ldur	q0, [x29, #-0x50]
  325f60:      	stp	x5, x16, [x9]
  325f64:      	stur	q0, [x9, #0x38]
  325f68:      	str	x26, [x9, #0x10]
  325f6c:      	stur	q2, [x9, #0x48]
  325f70:      	stp	x23, x4, [x9, #0x58]
  325f74:      	str	x3, [x9, #0x68]
  325f78:      	ldr	q0, [sp, #0x70]
  325f7c:      	str	q0, [x9, #0x70]
  325f80:      	ldrb	w10, [sp, #0x80]
  325f84:      	strb	w10, [x9, #0x80]
  325f88:      	strb	w17, [x9, #0x81]
  325f8c:      	strb	w18, [x9, #0x82]
  325f90:      	strb	w1, [x9, #0x83]
  325f94:      	stp	w2, wzr, [x9, #0x84]
  325f98:      	add	x20, x20, #0x20
  325f9c:      	strb	w24, [x9, #0x90]
  325fa0:      	add	x9, x27, #0x1
  325fa4:      	str	x9, [x0, #0x3d8]
  325fa8:      	subs	x21, x21, #0x20
  325fac:      	b.eq	0x326498 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x678>
  325fb0:      	ldr	w19, [x20, #0x4]
  325fb4:      	ldr	x1, [x0, #0x228]
  325fb8:      	cmp	x1, x19
  325fbc:      	b.ls	0x3264ec <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6cc>
  325fc0:      	ldr	x9, [x0, #0x220]
  325fc4:      	umaddl	x10, w19, w13, x9
  325fc8:      	ldr	x5, [x10]
  325fcc:      	cmp	x5, #0x2
  325fd0:      	b.eq	0x326490 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x670>
  325fd4:      	mov	w9, wzr
  325fd8:      	ldp	x16, x26, [x10, #0x8]
  325fdc:      	ldur	q0, [x10, #0x18]
  325fe0:      	ldur	q1, [x10, #0x28]
  325fe4:      	stp	q0, q1, [x29, #-0x40]
  325fe8:      	ldur	q2, [x10, #0x48]
  325fec:      	ldr	x27, [x10, #0x48]
  325ff0:      	ldp	x4, x3, [x10, #0x60]
  325ff4:      	ldrb	w17, [x10, #0x81]
  325ff8:      	ldrb	w18, [x10, #0x82]
  325ffc:      	ldrb	w1, [x10, #0x83]
  326000:      	ldur	q0, [x10, #0x38]
  326004:      	stur	q0, [x29, #-0x50]
  326008:      	ldr	w2, [x10, #0x84]
  32600c:      	ldr	q0, [x10, #0x70]
  326010:      	ldrb	w11, [x10, #0x80]
  326014:      	strb	w11, [sp, #0x80]
  326018:      	ldr	x23, [x10, #0x58]
  32601c:      	str	q0, [sp, #0x70]
  326020:      	cbz	x27, 0x3261ec <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3cc>
  326024:      	cmp	x23, x27
  326028:      	b.ne	0x3261ec <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3cc>
  32602c:      	sub	w9, w1, #0x3
  326030:      	cmp	w9, #0x2
  326034:      	b.lo	0x32604c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x22c>
  326038:      	cmp	w1, #0x1
  32603c:      	b.eq	0x32604c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x22c>
  326040:      	cmp	w1, #0x2
  326044:      	b.ne	0x326320 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x500>
  326048:      	tbz	w17, #0x0, 0x326320 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x500>
  32604c:      	adds	x4, x4, #0x1
  326050:      	b.hs	0x3264d4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6b4>
  326054:      	ldr	x16, [x0, #0x420]
  326058:      	ldr	x24, [x0, #0x3f0]
  32605c:      	ldr	x9, [x0, #0x208]
  326060:      	cmp	x24, x9
  326064:      	b.hs	0x3261c0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3a0>
  326068:      	ldr	w23, [x20]
  32606c:      	ldr	x9, [sp, #0x20]
  326070:      	ldr	x9, [x9]
  326074:      	cmp	x24, x9
  326078:      	b.ne	0x3260e0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x2c0>
  32607c:      	stp	x21, x20, [sp, #0x40]
  326080:      	mov	x28, x0
  326084:      	add	x0, x0, #0x3e0
  326088:      	mov	x25, x8
  32608c:      	stur	x16, [x29, #-0x8]
  326090:      	mov	w20, w17
  326094:      	stp	w2, w18, [sp, #0x68]
  326098:      	mov	w21, w1
  32609c:      	mov	x22, x3
  3260a0:      	str	q2, [sp, #0x50]
  3260a4:      	str	x4, [sp, #0x28]
  3260a8:      	bl	0x355920 <_RNvMs4_NtCseGoURbGxTjs_5alloc7raw_vecINtB5_6RawVecNtNtCsetTf0lGzr35_11shoop_loops8decision5EventE8grow_oneBQ_>
  3260ac:      	ldr	q2, [sp, #0x50]
  3260b0:      	mov	x3, x22
  3260b4:      	ldp	w2, w18, [sp, #0x68]
  3260b8:      	mov	w1, w21
  3260bc:      	mov	w17, w20
  3260c0:      	ldur	x16, [x29, #-0x8]
  3260c4:      	mov	w14, #0x98              // =152
  3260c8:      	mov	w13, #0x88              // =136
  3260cc:      	mov	x0, x28
  3260d0:      	ldp	x4, x28, [sp, #0x28]
  3260d4:      	ldp	x21, x20, [sp, #0x40]
  3260d8:      	ldr	x22, [sp, #0x38]
  3260dc:      	mov	x8, x25
  3260e0:      	ldr	x9, [x0, #0x3e8]
  3260e4:      	mov	w10, #0x30              // =48
  3260e8:      	madd	x9, x24, x10, x9
  3260ec:      	mov	w10, #0xd               // =13
  3260f0:      	strb	w10, [x9]
  3260f4:      	stp	w23, w19, [x9, #0x20]
  3260f8:      	str	x16, [x9, #0x28]
  3260fc:      	add	x19, x24, #0x1
  326100:      	str	x19, [x0, #0x3f0]
  326104:      	ldr	x9, [x0, #0x208]
  326108:      	cmp	x19, x9
  32610c:      	b.hs	0x3261d0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3b0>
  326110:      	ldr	d8, [x20]
  326114:      	ldr	x11, [x0, #0x420]
  326118:      	ldr	x9, [x0, #0x3e0]
  32611c:      	cmp	x19, x9
  326120:      	b.ne	0x32617c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x35c>
  326124:      	mov	x23, x0
  326128:      	add	x0, x0, #0x3e0
  32612c:      	mov	x25, x8
  326130:      	stur	x16, [x29, #-0x8]
  326134:      	str	w17, [sp, #0x48]
  326138:      	stp	w2, w18, [sp, #0x68]
  32613c:      	str	w1, [sp, #0x40]
  326140:      	stp	x11, x3, [sp, #0x8]
  326144:      	str	q2, [sp, #0x50]
  326148:      	str	x4, [sp, #0x28]
  32614c:      	bl	0x355920 <_RNvMs4_NtCseGoURbGxTjs_5alloc7raw_vecINtB5_6RawVecNtNtCsetTf0lGzr35_11shoop_loops8decision5EventE8grow_oneBQ_>
  326150:      	ldp	x11, x3, [sp, #0x8]
  326154:      	ldr	x4, [sp, #0x28]
  326158:      	ldr	q2, [sp, #0x50]
  32615c:      	ldp	w2, w18, [sp, #0x68]
  326160:      	ldr	w1, [sp, #0x40]
  326164:      	ldr	w17, [sp, #0x48]
  326168:      	ldur	x16, [x29, #-0x8]
  32616c:      	mov	w14, #0x98              // =152
  326170:      	mov	w13, #0x88              // =136
  326174:      	mov	x0, x23
  326178:      	mov	x8, x25
  32617c:      	mov	x23, xzr
  326180:      	ldr	x9, [x0, #0x3e8]
  326184:      	mov	w10, #0x30              // =48
  326188:      	madd	x9, x19, x10, x9
  32618c:      	mov	w10, #0xe               // =14
  326190:      	strb	w10, [x9]
  326194:      	str	d8, [x9, #0x20]
  326198:      	str	x11, [x9, #0x28]
  32619c:      	add	x9, x24, #0x2
  3261a0:      	str	x9, [x0, #0x3f0]
  3261a4:      	mov	w5, #0x1                // =1
  3261a8:      	mov	w9, w5
  3261ac:      	orr	x10, x3, x27
  3261b0:      	orr	x10, x10, xzr
  3261b4:      	tst	x10, x22
  3261b8:      	b.eq	0x3261fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3dc>
  3261bc:      	b	0x326338 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x518>
  3261c0:      	ldr	x9, [x0, #0x3f8]
  3261c4:      	adds	x9, x9, #0x1
  3261c8:      	csinv	x9, x9, xzr, lo
  3261cc:      	str	x9, [x0, #0x3f8]
  3261d0:      	mov	x23, xzr
  3261d4:      	ldr	x9, [x0, #0x3f8]
  3261d8:      	adds	x9, x9, #0x1
  3261dc:      	csinv	x9, x9, xzr, lo
  3261e0:      	str	x9, [x0, #0x3f8]
  3261e4:      	mov	w9, #0x1                // =1
  3261e8:      	mov	w5, #0x1                // =1
  3261ec:      	orr	x10, x3, x27
  3261f0:      	orr	x10, x10, x23
  3261f4:      	tst	x10, x22
  3261f8:      	b.ne	0x326338 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x518>
  3261fc:      	cmp	x26, #0x2
  326200:      	ccmp	x23, x27, #0x2, eq
  326204:      	cset	w10, lo
  326208:      	b.hs	0x326230 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x410>
  32620c:      	sub	w11, w1, #0x3
  326210:      	cmp	w11, #0x3
  326214:      	b.lo	0x326240 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x420>
  326218:      	cmp	w1, #0x1
  32621c:      	b.eq	0x326240 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x420>
  326220:      	cmp	w1, #0x2
  326224:      	b.ne	0x32626c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x44c>
  326228:      	tbnz	w17, #0x0, 0x326240 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x420>
  32622c:      	b	0x326264 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x444>
  326230:      	cmp	w1, #0x5
  326234:      	b.ne	0x326254 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x434>
  326238:      	cmp	x23, x27
  32623c:      	b.hs	0x326254 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x434>
  326240:      	adds	x11, x23, x28
  326244:      	cset	w12, hs
  326248:      	cmp	x11, x27
  32624c:      	csinc	w11, w12, wzr, ls
  326250:      	tbnz	w11, #0x0, 0x326338 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x518>
  326254:      	cmp	w1, #0x4
  326258:      	b.eq	0x326264 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x444>
  32625c:      	cmp	w1, #0x2
  326260:      	b.ne	0x32626c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x44c>
  326264:      	cmn	x3, x28
  326268:      	b.hs	0x3264d4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6b4>
  32626c:      	and	w9, w9, w10
  326270:      	tbz	w9, #0x0, 0x32629c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x47c>
  326274:      	sub	w9, w1, #0x3
  326278:      	cmp	w9, #0x2
  32627c:      	b.lo	0x326294 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x474>
  326280:      	cmp	w1, #0x2
  326284:      	mov	w24, w17
  326288:      	b.eq	0x3262a0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x480>
  32628c:      	cmp	w1, #0x1
  326290:      	b.ne	0x32629c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x47c>
  326294:      	mov	w24, #0x1               // =1
  326298:      	b	0x3262a0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x480>
  32629c:      	mov	w24, wzr
  3262a0:      	ldr	x27, [x0, #0x3d8]
  3262a4:      	ldr	x9, [x0, #0x3c8]
  3262a8:      	cmp	x27, x9
  3262ac:      	b.ne	0x325f48 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x128>
  3262b0:      	stp	x21, x20, [sp, #0x40]
  3262b4:      	mov	x28, x0
  3262b8:      	add	x0, x0, #0x3c8
  3262bc:      	mov	x25, x8
  3262c0:      	stur	x16, [x29, #-0x8]
  3262c4:      	mov	w19, w17
  3262c8:      	stp	w2, w18, [sp, #0x68]
  3262cc:      	mov	w20, w1
  3262d0:      	mov	x21, x3
  3262d4:      	str	q2, [sp, #0x50]
  3262d8:      	mov	x22, x4
  3262dc:      	str	x5, [sp, #0x28]
  3262e0:      	bl	0x355990 <_RNvMs4_NtCseGoURbGxTjs_5alloc7raw_vecINtB5_6RawVecNtNtCsetTf0lGzr35_11shoop_loops8decision8DecisionE8grow_oneBQ_>
  3262e4:      	mov	x4, x22
  3262e8:      	ldr	q2, [sp, #0x50]
  3262ec:      	mov	x3, x21
  3262f0:      	ldp	w2, w18, [sp, #0x68]
  3262f4:      	mov	w1, w20
  3262f8:      	mov	w17, w19
  3262fc:      	ldur	x16, [x29, #-0x8]
  326300:      	mov	w14, #0x98              // =152
  326304:      	mov	w13, #0x88              // =136
  326308:      	mov	x0, x28
  32630c:      	ldp	x5, x28, [sp, #0x28]
  326310:      	ldp	x21, x20, [sp, #0x40]
  326314:      	ldr	x22, [sp, #0x38]
  326318:      	mov	x8, x25
  32631c:      	b	0x325f48 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x128>
  326320:      	mov	w9, wzr
  326324:      	mov	x23, x27
  326328:      	orr	x10, x3, x27
  32632c:      	orr	x10, x10, x27
  326330:      	tst	x10, x22
  326334:      	b.eq	0x3261fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3dc>
  326338:      	mov	w9, #0x5                // =5
  32633c:      	strb	w9, [x8, #0x8]
  326340:      	mov	w9, #0x2                // =2
  326344:      	str	x9, [x8]
  326348:      	b	0x32646c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x64c>
  32634c:      	ldp	x20, x10, [x0, #0x140]
  326350:      	cbz	x10, 0x3263ac <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x58c>
  326354:      	lsl	x11, x10, #5
  326358:      	ldr	x1, [x0, #0x228]
  32635c:      	ldr	x12, [x0, #0x220]
  326360:      	add	x13, x20, #0x18
  326364:      	mov	w14, #0x88              // =136
  326368:      	ldrb	w9, [x13]
  32636c:      	tbnz	w9, #0x0, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  326370:      	ldur	w9, [x13, #-0x14]
  326374:      	cmp	x1, x9
  326378:      	b.ls	0x3264fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6dc>
  32637c:      	umaddl	x9, w9, w14, x12
  326380:      	ldr	x15, [x9]
  326384:      	cmp	x15, #0x2
  326388:      	b.eq	0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  32638c:      	ldrb	w15, [x9, #0x83]
  326390:      	ldr	x9, [x9, #0x10]
  326394:      	cmp	x9, #0x2
  326398:      	ccmp	w15, #0x2, #0x2, eq
  32639c:      	b.hi	0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  3263a0:      	add	x13, x13, #0x20
  3263a4:      	subs	x11, x11, #0x20
  3263a8:      	b.ne	0x326368 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x548>
  3263ac:      	mov	w9, #0x1                // =1
  3263b0:      	strb	w9, [x0, #0x432]
  3263b4:      	lsl	x21, x10, #5
  3263b8:      	cbnz	x10, 0x325e80 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x60>
  3263bc:      	b	0x325ebc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  3263c0:      	add	x11, x20, x21
  3263c4:      	ldr	w12, [x9, #0x10]
  3263c8:      	cbnz	w12, 0x3263f4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5d4>
  3263cc:      	ldr	w12, [x9, #0x30]
  3263d0:      	cbnz	w12, 0x3263f4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5d4>
  3263d4:      	ldr	w12, [x9, #0x50]
  3263d8:      	cbnz	w12, 0x3263f4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5d4>
  3263dc:      	ldr	w12, [x9, #0x70]
  3263e0:      	cbnz	w12, 0x3263f4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5d4>
  3263e4:      	add	x9, x9, #0x80
  3263e8:      	cmp	x9, x11
  3263ec:      	b.ne	0x3263c4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5a4>
  3263f0:      	b	0x325ebc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  3263f4:      	ldr	x1, [x0, #0x228]
  3263f8:      	ldr	x11, [x0, #0x220]
  3263fc:      	add	x12, x20, #0x4
  326400:      	mov	w13, #0x88              // =136
  326404:      	mov	x14, x21
  326408:      	b	0x32641c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5fc>
  32640c:      	ldrb	w9, [x9, #0x81]
  326410:      	tbnz	w9, #0x0, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x648>
  326414:      	subs	x14, x14, #0x20
  326418:      	b.eq	0x325ebc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  32641c:      	ldr	w9, [x12], #0x20
  326420:      	cmp	x1, x9
  326424:      	b.ls	0x3264dc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6bc>
  326428:      	umaddl	x9, w9, w13, x11
  32642c:      	ldr	x15, [x9]
  326430:      	cmp	x15, #0x2
  326434:      	b.eq	0x3264bc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x69c>
  326438:      	ldr	x15, [x9, #0x58]
  32643c:      	ldr	x16, [x9, #0x48]
  326440:      	cmp	x16, #0x0
  326444:      	ccmp	x15, x16, #0x0, ne
  326448:      	b.ne	0x326414 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5f4>
  32644c:      	ldrb	w15, [x9, #0x83]
  326450:      	cmp	w15, #0x2
  326454:      	b.eq	0x32640c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5ec>
  326458:      	sub	w9, w15, #0x3
  32645c:      	cmp	w9, #0x2
  326460:      	ccmp	w15, #0x1, #0x4, hs
  326464:      	b.ne	0x326414 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5f4>
  326468:      	str	xzr, [x8]
  32646c:      	ldp	x20, x19, [sp, #0x130]
  326470:      	ldp	x22, x21, [sp, #0x120]
  326474:      	ldp	x24, x23, [sp, #0x110]
  326478:      	ldp	x26, x25, [sp, #0x100]
  32647c:      	ldp	x28, x27, [sp, #0xf0]
  326480:      	ldp	x29, x30, [sp, #0xe0]
  326484:      	ldr	d8, [sp, #0xd0]
  326488:      	add	sp, sp, #0x140
  32648c:      	ret
  326490:      	mov	w9, #0x4                // =4
  326494:      	b	0x32633c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x51c>
  326498:      	ldr	x9, [x0, #0x420]
  32649c:      	ldr	x11, [sp, #0x18]
  3264a0:      	mov	w10, #0x1               // =1
  3264a4:      	stp	x10, x9, [x0]
  3264a8:      	stp	x11, xzr, [x0, #0x10]
  3264ac:      	strb	w10, [x0, #0x433]
  3264b0:      	stp	x10, x9, [x8]
  3264b4:      	str	x11, [x8, #0x10]
  3264b8:      	b	0x32646c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x64c>
  3264bc:      	adrp	x0, 0x211000 <anon.4fb15d12279b4f77fa16400b3daf47d2.13.llvm.5598435376432120934+0x680>
  3264c0:      	add	x0, x0, #0xed
  3264c4:      	adrp	x2, 0x449000 <anon.10e916c18a1ed16150ef4d4cc9e29fff.5.llvm.1869985738199924937+0xd8>
  3264c8:      	add	x2, x2, #0xf50
  3264cc:      	mov	w1, #0xc                // =12
  3264d0:      	bl	0x42ad08 <_RNvNtCs8l20omZJ7ql_4core6option13expect_failed>
  3264d4:      	mov	w9, #0xa                // =10
  3264d8:      	b	0x32633c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x51c>
  3264dc:      	adrp	x2, 0x449000 <anon.10e916c18a1ed16150ef4d4cc9e29fff.5.llvm.1869985738199924937+0xd8>
  3264e0:      	add	x2, x2, #0xf38
  3264e4:      	mov	x0, x9
  3264e8:      	bl	0x42add8 <_RNvNtCs8l20omZJ7ql_4core9panicking18panic_bounds_check>
  3264ec:      	adrp	x2, 0x44a000 <anon.ffeccae1148640ba9aba89f0857e490a.73.llvm.6935176436661636159+0xb20>
  3264f0:      	add	x2, x2, #0xa8
  3264f4:      	mov	x0, x19
  3264f8:      	bl	0x42add8 <_RNvNtCs8l20omZJ7ql_4core9panicking18panic_bounds_check>
  3264fc:      	adrp	x2, 0x449000 <anon.10e916c18a1ed16150ef4d4cc9e29fff.5.llvm.1869985738199924937+0xd8>
  326500:      	add	x2, x2, #0xf68
  326504:      	mov	x0, x9
  326508:      	bl	0x42add8 <_RNvNtCs8l20omZJ7ql_4core9panicking18panic_bounds_check>
		...
