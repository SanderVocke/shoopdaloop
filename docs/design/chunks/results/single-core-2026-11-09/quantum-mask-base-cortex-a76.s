
work/pi5-audio-bench/audio-bench-packed-descriptors:	file format elf64-littleaarch64

Disassembly of section .text:

0000000000325fc0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick>:
  325fc0:      	sub	sp, sp, #0x130
  325fc4:      	str	d8, [sp, #0xc0]
  325fc8:      	stp	x29, x30, [sp, #0xd0]
  325fcc:      	stp	x28, x27, [sp, #0xe0]
  325fd0:      	stp	x26, x25, [sp, #0xf0]
  325fd4:      	stp	x24, x23, [sp, #0x100]
  325fd8:      	stp	x22, x21, [sp, #0x110]
  325fdc:      	stp	x20, x19, [sp, #0x120]
  325fe0:      	add	x29, sp, #0xd0
  325fe4:      	cbnz	x2, 0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  325fe8:      	ldrb	w9, [x0, #0x435]
  325fec:      	tbnz	w9, #0x0, 0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  325ff0:      	ldrb	w9, [x0, #0x436]
  325ff4:      	tbnz	w9, #0x0, 0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  325ff8:      	ldr	x9, [x0, #0x28]
  325ffc:      	cmp	x9, #0x2
  326000:      	b.ne	0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  326004:      	ldrb	w9, [x0, #0x434]
  326008:      	tbnz	w9, #0x0, 0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  32600c:      	ldrb	w9, [x0, #0x432]
  326010:      	cbz	w9, 0x326090 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0xd0>
  326014:      	ldp	x18, x10, [x0, #0x140]
  326018:      	lsl	x17, x10, #5
  32601c:      	cbz	x10, 0x32605c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  326020:      	sub	x11, x17, #0x20
  326024:      	lsr	w9, w11, #5
  326028:      	add	w9, w9, #0x1
  32602c:      	ands	x12, x9, #0x3
  326030:      	mov	x9, x18
  326034:      	b.eq	0x326054 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x94>
  326038:      	lsl	x12, x12, #5
  32603c:      	mov	x9, x18
  326040:      	ldr	w13, [x9, #0x10]
  326044:      	cbnz	w13, 0x3265b8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5f8>
  326048:      	add	x9, x9, #0x20
  32604c:      	subs	x12, x12, #0x20
  326050:      	b.ne	0x326040 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x80>
  326054:      	cmp	x11, #0x60
  326058:      	b.hs	0x326584 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5c4>
  32605c:      	ldr	x9, [x0, #0x420]
  326060:      	ldrh	w2, [x0, #0x430]
  326064:      	adds	x11, x9, x2
  326068:      	b.hs	0x3266a8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6e8>
  32606c:      	str	xzr, [x0, #0x3d8]
  326070:      	movi	v0.2d, #0000000000000000
  326074:      	str	q0, [x0, #0x3f0]
  326078:      	cbz	x10, 0x326674 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6b4>
  32607c:      	add	x9, x0, #0x3e0
  326080:      	stp	x11, x9, [sp, #0x10]
  326084:      	mov	w28, #0x88              // =136
  326088:      	mov	w13, #0x98              // =152
  32608c:      	b	0x32613c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x17c>
  326090:      	ldr	x9, [x0, #0x160]
  326094:      	cbnz	x9, 0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  326098:      	ldr	x10, [x0, #0x258]
  32609c:      	cbz	x10, 0x326510 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x550>
  3260a0:      	ldr	x9, [x0, #0x250]
  3260a4:      	lsl	x10, x10, #7
  3260a8:      	b	0x3260b8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0xf8>
  3260ac:      	add	x9, x9, #0x80
  3260b0:      	subs	x10, x10, #0x80
  3260b4:      	b.eq	0x326510 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x550>
  3260b8:      	ldrb	w11, [x9, #0x78]
  3260bc:      	cmp	w11, #0x2
  3260c0:      	b.eq	0x3260ac <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0xec>
  3260c4:      	ldrb	w11, [x9]
  3260c8:      	cmp	w11, #0xff
  3260cc:      	b.ne	0x3260ac <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0xec>
  3260d0:      	b	0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  3260d4:      	ldp	q0, q1, [x29, #-0x40]
  3260d8:      	ldr	x9, [x0, #0x3d0]
  3260dc:      	madd	x9, x24, x13, x9
  3260e0:      	stur	q0, [x9, #0x18]
  3260e4:      	stur	q1, [x9, #0x28]
  3260e8:      	ldur	q0, [x29, #-0x50]
  3260ec:      	stp	x25, x15, [x9]
  3260f0:      	stur	q0, [x9, #0x38]
  3260f4:      	str	x14, [x9, #0x10]
  3260f8:      	stur	q2, [x9, #0x48]
  3260fc:      	stp	x27, x6, [x9, #0x58]
  326100:      	str	x5, [x9, #0x68]
  326104:      	ldr	q0, [sp, #0x60]
  326108:      	str	q0, [x9, #0x70]
  32610c:      	ldrb	w10, [sp, #0x70]
  326110:      	strb	w10, [x9, #0x80]
  326114:      	strb	w16, [x9, #0x81]
  326118:      	strb	w1, [x9, #0x82]
  32611c:      	strb	w3, [x9, #0x83]
  326120:      	stp	w4, wzr, [x9, #0x84]
  326124:      	add	x18, x18, #0x20
  326128:      	strb	w11, [x9, #0x90]
  32612c:      	add	x9, x24, #0x1
  326130:      	str	x9, [x0, #0x3d8]
  326134:      	subs	x17, x17, #0x20
  326138:      	b.eq	0x32666c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6ac>
  32613c:      	ldr	w19, [x18, #0x4]
  326140:      	ldr	x1, [x0, #0x228]
  326144:      	cmp	x1, x19
  326148:      	b.ls	0x3266c0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x700>
  32614c:      	ldr	x9, [x0, #0x220]
  326150:      	umaddl	x10, w19, w28, x9
  326154:      	ldr	x25, [x10]
  326158:      	cmp	x25, #0x2
  32615c:      	b.eq	0x326664 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6a4>
  326160:      	mov	w9, wzr
  326164:      	ldp	x15, x14, [x10, #0x8]
  326168:      	ldur	q0, [x10, #0x18]
  32616c:      	ldur	q1, [x10, #0x28]
  326170:      	stp	q0, q1, [x29, #-0x40]
  326174:      	ldur	q2, [x10, #0x48]
  326178:      	ldr	x24, [x10, #0x48]
  32617c:      	ldp	x6, x5, [x10, #0x60]
  326180:      	ldrb	w16, [x10, #0x81]
  326184:      	ldrb	w1, [x10, #0x82]
  326188:      	ldrb	w3, [x10, #0x83]
  32618c:      	ldur	q0, [x10, #0x38]
  326190:      	stur	q0, [x29, #-0x50]
  326194:      	ldr	w4, [x10, #0x84]
  326198:      	ldr	q0, [x10, #0x70]
  32619c:      	ldrb	w11, [x10, #0x80]
  3261a0:      	strb	w11, [sp, #0x70]
  3261a4:      	ldr	x27, [x10, #0x58]
  3261a8:      	str	q0, [sp, #0x60]
  3261ac:      	cbz	x24, 0x3263a0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3e0>
  3261b0:      	cmp	x27, x24
  3261b4:      	b.ne	0x3263a0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3e0>
  3261b8:      	sub	w9, w3, #0x3
  3261bc:      	cmp	w9, #0x2
  3261c0:      	b.lo	0x3261d8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x218>
  3261c4:      	cmp	w3, #0x1
  3261c8:      	b.eq	0x3261d8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x218>
  3261cc:      	cmp	w3, #0x2
  3261d0:      	b.ne	0x326398 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3d8>
  3261d4:      	tbz	w16, #0x0, 0x326398 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3d8>
  3261d8:      	adds	x6, x6, #0x1
  3261dc:      	b.hs	0x3266a8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6e8>
  3261e0:      	ldr	x15, [x0, #0x420]
  3261e4:      	ldr	x26, [x0, #0x3f0]
  3261e8:      	ldr	x9, [x0, #0x208]
  3261ec:      	cmp	x26, x9
  3261f0:      	b.hs	0x326368 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3a8>
  3261f4:      	ldr	w27, [x18]
  3261f8:      	ldr	x9, [sp, #0x18]
  3261fc:      	ldr	x9, [x9]
  326200:      	cmp	x26, x9
  326204:      	b.ne	0x326278 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x2b8>
  326208:      	mov	x21, x0
  32620c:      	add	x0, x0, #0x3e0
  326210:      	mov	x22, x8
  326214:      	mov	x28, x17
  326218:      	stp	x15, x14, [sp, #0x28]
  32621c:      	mov	x20, x18
  326220:      	mov	w25, w16
  326224:      	str	w1, [sp, #0x20]
  326228:      	mov	x23, x2
  32622c:      	stp	w4, w3, [x29, #-0x8]
  326230:      	str	x5, [sp, #0x58]
  326234:      	str	q2, [sp, #0x40]
  326238:      	str	x6, [sp, #0x38]
  32623c:      	bl	0x355b00 <_RNvMs4_NtCseGoURbGxTjs_5alloc7raw_vecINtB5_6RawVecNtNtCsetTf0lGzr35_11shoop_loops8decision5EventE8grow_oneBQ_>
  326240:      	ldp	x14, x6, [sp, #0x30]
  326244:      	ldr	q2, [sp, #0x40]
  326248:      	ldr	x5, [sp, #0x58]
  32624c:      	ldp	w4, w3, [x29, #-0x8]
  326250:      	mov	x2, x23
  326254:      	ldr	w1, [sp, #0x20]
  326258:      	mov	w16, w25
  32625c:      	ldr	x15, [sp, #0x28]
  326260:      	mov	x18, x20
  326264:      	mov	w13, #0x98              // =152
  326268:      	mov	x17, x28
  32626c:      	mov	w28, #0x88              // =136
  326270:      	mov	x0, x21
  326274:      	mov	x8, x22
  326278:      	ldr	x9, [x0, #0x3e8]
  32627c:      	mov	w10, #0x30              // =48
  326280:      	madd	x9, x26, x10, x9
  326284:      	mov	w10, #0xd               // =13
  326288:      	strb	w10, [x9]
  32628c:      	stp	w27, w19, [x9, #0x20]
  326290:      	str	x15, [x9, #0x28]
  326294:      	add	x19, x26, #0x1
  326298:      	str	x19, [x0, #0x3f0]
  32629c:      	ldr	x9, [x0, #0x208]
  3262a0:      	cmp	x19, x9
  3262a4:      	b.hs	0x326378 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3b8>
  3262a8:      	ldr	d8, [x18]
  3262ac:      	ldr	x11, [x0, #0x420]
  3262b0:      	ldr	x9, [x0, #0x3e0]
  3262b4:      	cmp	x19, x9
  3262b8:      	b.ne	0x326334 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x374>
  3262bc:      	mov	x21, x0
  3262c0:      	add	x0, x0, #0x3e0
  3262c4:      	mov	x22, x8
  3262c8:      	mov	x27, x17
  3262cc:      	mov	x20, x14
  3262d0:      	str	x18, [sp, #0x30]
  3262d4:      	mov	x25, x15
  3262d8:      	str	w16, [sp, #0x28]
  3262dc:      	mov	w23, w1
  3262e0:      	str	x2, [sp, #0x20]
  3262e4:      	stp	w4, w3, [x29, #-0x8]
  3262e8:      	str	x5, [sp, #0x58]
  3262ec:      	str	q2, [sp, #0x40]
  3262f0:      	str	x6, [sp, #0x38]
  3262f4:      	str	x11, [sp, #0x8]
  3262f8:      	bl	0x355b00 <_RNvMs4_NtCseGoURbGxTjs_5alloc7raw_vecINtB5_6RawVecNtNtCsetTf0lGzr35_11shoop_loops8decision5EventE8grow_oneBQ_>
  3262fc:      	ldr	x11, [sp, #0x8]
  326300:      	ldp	x18, x6, [sp, #0x30]
  326304:      	ldr	q2, [sp, #0x40]
  326308:      	ldr	x5, [sp, #0x58]
  32630c:      	ldp	w4, w3, [x29, #-0x8]
  326310:      	ldr	x2, [sp, #0x20]
  326314:      	mov	w1, w23
  326318:      	ldr	w16, [sp, #0x28]
  32631c:      	mov	x15, x25
  326320:      	mov	x14, x20
  326324:      	mov	w13, #0x98              // =152
  326328:      	mov	x17, x27
  32632c:      	mov	x0, x21
  326330:      	mov	x8, x22
  326334:      	mov	x27, xzr
  326338:      	ldr	x9, [x0, #0x3e8]
  32633c:      	mov	w10, #0x30              // =48
  326340:      	madd	x9, x19, x10, x9
  326344:      	mov	w10, #0xe               // =14
  326348:      	strb	w10, [x9]
  32634c:      	str	d8, [x9, #0x20]
  326350:      	str	x11, [x9, #0x28]
  326354:      	add	x9, x26, #0x2
  326358:      	str	x9, [x0, #0x3f0]
  32635c:      	mov	w25, #0x1               // =1
  326360:      	mov	w9, w25
  326364:      	b	0x3263a0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3e0>
  326368:      	ldr	x9, [x0, #0x3f8]
  32636c:      	adds	x9, x9, #0x1
  326370:      	csinv	x9, x9, xzr, lo
  326374:      	str	x9, [x0, #0x3f8]
  326378:      	mov	x27, xzr
  32637c:      	ldr	x9, [x0, #0x3f8]
  326380:      	adds	x9, x9, #0x1
  326384:      	csinv	x9, x9, xzr, lo
  326388:      	str	x9, [x0, #0x3f8]
  32638c:      	mov	w9, #0x1                // =1
  326390:      	mov	w25, #0x1               // =1
  326394:      	b	0x3263a0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x3e0>
  326398:      	mov	w9, wzr
  32639c:      	mov	x27, x24
  3263a0:      	cbz	w2, 0x3266d0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x710>
  3263a4:      	udiv	x10, x24, x2
  3263a8:      	msub	x10, x10, x2, x24
  3263ac:      	cbnz	x10, 0x3264fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x53c>
  3263b0:      	udiv	x10, x27, x2
  3263b4:      	msub	x10, x10, x2, x27
  3263b8:      	cbnz	x10, 0x3264fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x53c>
  3263bc:      	udiv	x10, x5, x2
  3263c0:      	msub	x10, x10, x2, x5
  3263c4:      	cbnz	x10, 0x3264fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x53c>
  3263c8:      	cmp	x14, #0x2
  3263cc:      	ccmp	x27, x24, #0x2, eq
  3263d0:      	cset	w10, lo
  3263d4:      	b.hs	0x3263fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x43c>
  3263d8:      	sub	w11, w3, #0x3
  3263dc:      	cmp	w11, #0x3
  3263e0:      	b.lo	0x32640c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x44c>
  3263e4:      	cmp	w3, #0x1
  3263e8:      	b.eq	0x32640c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x44c>
  3263ec:      	cmp	w3, #0x2
  3263f0:      	b.ne	0x326438 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x478>
  3263f4:      	tbnz	w16, #0x0, 0x32640c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x44c>
  3263f8:      	b	0x326430 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x470>
  3263fc:      	cmp	w3, #0x5
  326400:      	b.ne	0x326420 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x460>
  326404:      	cmp	x27, x24
  326408:      	b.hs	0x326420 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x460>
  32640c:      	adds	x11, x27, x2
  326410:      	cset	w12, hs
  326414:      	cmp	x11, x24
  326418:      	csinc	w11, w12, wzr, ls
  32641c:      	tbnz	w11, #0x0, 0x3264fc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x53c>
  326420:      	cmp	w3, #0x4
  326424:      	b.eq	0x326430 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x470>
  326428:      	cmp	w3, #0x2
  32642c:      	b.ne	0x326438 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x478>
  326430:      	cmn	x5, x2
  326434:      	b.hs	0x3266a8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6e8>
  326438:      	and	w9, w9, w10
  32643c:      	tbz	w9, #0x0, 0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x4a8>
  326440:      	sub	w9, w3, #0x3
  326444:      	cmp	w9, #0x2
  326448:      	b.lo	0x326460 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x4a0>
  32644c:      	cmp	w3, #0x2
  326450:      	mov	w11, w16
  326454:      	b.eq	0x32646c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x4ac>
  326458:      	cmp	w3, #0x1
  32645c:      	b.ne	0x326468 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x4a8>
  326460:      	mov	w11, #0x1               // =1
  326464:      	b	0x32646c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x4ac>
  326468:      	mov	w11, wzr
  32646c:      	ldr	x24, [x0, #0x3d8]
  326470:      	ldr	x9, [x0, #0x3c8]
  326474:      	cmp	x24, x9
  326478:      	b.ne	0x3260d4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x114>
  32647c:      	mov	x21, x0
  326480:      	add	x0, x0, #0x3c8
  326484:      	mov	x22, x8
  326488:      	mov	x28, x17
  32648c:      	stp	x15, x14, [sp, #0x28]
  326490:      	mov	x19, x18
  326494:      	mov	w26, w16
  326498:      	str	w1, [sp, #0x20]
  32649c:      	mov	x20, x2
  3264a0:      	stur	w3, [x29, #-0x4]
  3264a4:      	mov	w23, w4
  3264a8:      	str	x5, [sp, #0x58]
  3264ac:      	str	q2, [sp, #0x40]
  3264b0:      	str	x6, [sp, #0x38]
  3264b4:      	stur	w11, [x29, #-0x8]
  3264b8:      	bl	0x355b70 <_RNvMs4_NtCseGoURbGxTjs_5alloc7raw_vecINtB5_6RawVecNtNtCsetTf0lGzr35_11shoop_loops8decision8DecisionE8grow_oneBQ_>
  3264bc:      	ldp	w11, w3, [x29, #-0x8]
  3264c0:      	ldp	x14, x6, [sp, #0x30]
  3264c4:      	ldr	q2, [sp, #0x40]
  3264c8:      	ldr	x5, [sp, #0x58]
  3264cc:      	mov	w4, w23
  3264d0:      	mov	x2, x20
  3264d4:      	ldr	w1, [sp, #0x20]
  3264d8:      	mov	w16, w26
  3264dc:      	ldr	x15, [sp, #0x28]
  3264e0:      	mov	x18, x19
  3264e4:      	mov	w13, #0x98              // =152
  3264e8:      	mov	x17, x28
  3264ec:      	mov	w28, #0x88              // =136
  3264f0:      	mov	x0, x21
  3264f4:      	mov	x8, x22
  3264f8:      	b	0x3260d4 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x114>
  3264fc:      	mov	w9, #0x5                // =5
  326500:      	strb	w9, [x8, #0x8]
  326504:      	mov	w9, #0x2                // =2
  326508:      	str	x9, [x8]
  32650c:      	b	0x326640 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x680>
  326510:      	ldp	x18, x10, [x0, #0x140]
  326514:      	cbz	x10, 0x326570 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5b0>
  326518:      	lsl	x11, x10, #5
  32651c:      	ldr	x1, [x0, #0x228]
  326520:      	ldr	x12, [x0, #0x220]
  326524:      	add	x13, x18, #0x18
  326528:      	mov	w14, #0x88              // =136
  32652c:      	ldrb	w9, [x13]
  326530:      	tbnz	w9, #0x0, 0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  326534:      	ldur	w9, [x13, #-0x14]
  326538:      	cmp	x1, x9
  32653c:      	b.ls	0x3266dc <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x71c>
  326540:      	umaddl	x9, w9, w14, x12
  326544:      	ldr	x15, [x9]
  326548:      	cmp	x15, #0x2
  32654c:      	b.eq	0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  326550:      	ldrb	w15, [x9, #0x83]
  326554:      	ldr	x9, [x9, #0x10]
  326558:      	cmp	x9, #0x2
  32655c:      	ccmp	w15, #0x2, #0x2, eq
  326560:      	b.hi	0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  326564:      	add	x13, x13, #0x20
  326568:      	subs	x11, x11, #0x20
  32656c:      	b.ne	0x32652c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x56c>
  326570:      	mov	w9, #0x1                // =1
  326574:      	strb	w9, [x0, #0x432]
  326578:      	lsl	x17, x10, #5
  32657c:      	cbnz	x10, 0x326020 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x60>
  326580:      	b	0x32605c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  326584:      	add	x11, x18, x17
  326588:      	ldr	w12, [x9, #0x10]
  32658c:      	cbnz	w12, 0x3265b8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5f8>
  326590:      	ldr	w12, [x9, #0x30]
  326594:      	cbnz	w12, 0x3265b8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5f8>
  326598:      	ldr	w12, [x9, #0x50]
  32659c:      	cbnz	w12, 0x3265b8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5f8>
  3265a0:      	ldr	w12, [x9, #0x70]
  3265a4:      	cbnz	w12, 0x3265b8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5f8>
  3265a8:      	add	x9, x9, #0x80
  3265ac:      	cmp	x9, x11
  3265b0:      	b.ne	0x326588 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x5c8>
  3265b4:      	b	0x32605c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  3265b8:      	ldr	x1, [x0, #0x228]
  3265bc:      	ldr	x11, [x0, #0x220]
  3265c0:      	add	x12, x18, #0x4
  3265c4:      	mov	w13, #0x88              // =136
  3265c8:      	mov	x14, x17
  3265cc:      	b	0x3265f0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x630>
  3265d0:      	nop
  3265d4:      	nop
  3265d8:      	nop
  3265dc:      	nop
  3265e0:      	ldrb	w9, [x9, #0x81]
  3265e4:      	tbnz	w9, #0x0, 0x32663c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x67c>
  3265e8:      	subs	x14, x14, #0x20
  3265ec:      	b.eq	0x32605c <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x9c>
  3265f0:      	ldr	w9, [x12], #0x20
  3265f4:      	cmp	x1, x9
  3265f8:      	b.ls	0x3266b0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6f0>
  3265fc:      	umaddl	x9, w9, w13, x11
  326600:      	ldr	x15, [x9]
  326604:      	cmp	x15, #0x2
  326608:      	b.eq	0x326690 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x6d0>
  32660c:      	ldr	x15, [x9, #0x58]
  326610:      	ldr	x16, [x9, #0x48]
  326614:      	cmp	x16, #0x0
  326618:      	ccmp	x15, x16, #0x0, ne
  32661c:      	b.ne	0x3265e8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x628>
  326620:      	ldrb	w15, [x9, #0x83]
  326624:      	cmp	w15, #0x2
  326628:      	b.eq	0x3265e0 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x620>
  32662c:      	sub	w9, w15, #0x3
  326630:      	cmp	w9, #0x2
  326634:      	ccmp	w15, #0x1, #0x4, hs
  326638:      	b.ne	0x3265e8 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x628>
  32663c:      	str	xzr, [x8]
  326640:      	ldp	x20, x19, [sp, #0x120]
  326644:      	ldp	x22, x21, [sp, #0x110]
  326648:      	ldp	x24, x23, [sp, #0x100]
  32664c:      	ldp	x26, x25, [sp, #0xf0]
  326650:      	ldp	x28, x27, [sp, #0xe0]
  326654:      	ldp	x29, x30, [sp, #0xd0]
  326658:      	ldr	d8, [sp, #0xc0]
  32665c:      	add	sp, sp, #0x130
  326660:      	ret
  326664:      	mov	w9, #0x4                // =4
  326668:      	b	0x326500 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x540>
  32666c:      	ldr	x9, [x0, #0x420]
  326670:      	ldr	x11, [sp, #0x10]
  326674:      	mov	w10, #0x1               // =1
  326678:      	stp	x10, x9, [x0]
  32667c:      	stp	x11, xzr, [x0, #0x10]
  326680:      	strb	w10, [x0, #0x433]
  326684:      	stp	x10, x9, [x8]
  326688:      	str	x11, [x8, #0x10]
  32668c:      	b	0x326640 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x680>
  326690:      	adrp	x0, 0x211000 <anon.c651436264347c4bf8b662979a51b2af.13.llvm.18031291008763238346+0x618>
  326694:      	add	x0, x0, #0x12d
  326698:      	adrp	x2, 0x44a000 <anon.ffeccae1148640ba9aba89f0857e490a.73.llvm.945741504984406383+0x808>
  32669c:      	add	x2, x2, #0x280
  3266a0:      	mov	w1, #0xc                // =12
  3266a4:      	bl	0x42afa8 <_RNvNtCs8l20omZJ7ql_4core6option13expect_failed>
  3266a8:      	mov	w9, #0xa                // =10
  3266ac:      	b	0x326500 <_RNvMNtNtCsetTf0lGzr35_11shoop_loops8resolver7compactNtB4_8Resolver12compact_tick+0x540>
  3266b0:      	adrp	x2, 0x44a000 <anon.ffeccae1148640ba9aba89f0857e490a.73.llvm.945741504984406383+0x808>
  3266b4:      	add	x2, x2, #0x268
  3266b8:      	mov	x0, x9
  3266bc:      	bl	0x42b078 <_RNvNtCs8l20omZJ7ql_4core9panicking18panic_bounds_check>
  3266c0:      	adrp	x2, 0x44a000 <anon.ffeccae1148640ba9aba89f0857e490a.73.llvm.945741504984406383+0x808>
  3266c4:      	add	x2, x2, #0x3d8
  3266c8:      	mov	x0, x19
  3266cc:      	bl	0x42b078 <_RNvNtCs8l20omZJ7ql_4core9panicking18panic_bounds_check>
  3266d0:      	adrp	x0, 0x44a000 <anon.ffeccae1148640ba9aba89f0857e490a.73.llvm.945741504984406383+0x808>
  3266d4:      	add	x0, x0, #0x3f0
  3266d8:      	bl	0x42c12c <_RNvNtNtCs8l20omZJ7ql_4core9panicking11panic_const23panic_const_rem_by_zero>
  3266dc:      	adrp	x2, 0x44a000 <anon.ffeccae1148640ba9aba89f0857e490a.73.llvm.945741504984406383+0x808>
  3266e0:      	add	x2, x2, #0x298
  3266e4:      	mov	x0, x9
  3266e8:      	bl	0x42b078 <_RNvNtCs8l20omZJ7ql_4core9panicking18panic_bounds_check>
		...
