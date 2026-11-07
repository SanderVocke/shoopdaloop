// Canonical RT protocol v25 and worklet configuration v7 records.
const protocolVersion = 25;
const u32 = value => [value & 255, (value >>> 8) & 255, (value >>> 16) & 255, value >>> 24];
const u64 = value => [...u32(value), 0, 0, 0, 0];
const id = [...u32(0), ...u32(1)];
const float = value => Array.from(new Uint8Array(new Float32Array([value]).buffer));
function configuration() {
  const values = [7, 8000, 128, 16, 1, 2, 1, 0, 64, 0, 2, 2, 2, 2, 0, 0, 0, 0,
    512, 256, 64, 512, 32, 128, 8, 0, 0, 64, 64, 512, 2,
    16384, 16384, 4096, 4096, 8, 256, 1024, 512, 1, 2, 4, 2, 0, 0, 1, 1];
  const bytes = values.flatMap(u32);
  for (const names of [['in_1', 'in_2'], ['out_1', 'out_2'], ['keys'], []]) {
    bytes.push(...u32(names.length));
    for (const name of names) bytes.push(name.length, ...new TextEncoder().encode(name));
  }
  return new Uint8Array(bytes).buffer;
}

export async function run() {
  if (crossOriginIsolated) throw new Error('acceptance requires ordinary non-shared worklet memory');
  const context = new OfflineAudioContext(2, 128 * 2048, 8000);
  await context.audioWorklet.addModule('/worklet.js');
  const wasmModule = await WebAssembly.compileStreaming(fetch('/engine.wasm'));
  const node = new AudioWorkletNode(context, 'shoop-rt-audio-processor', {
    numberOfInputs: 1, numberOfOutputs: 1, channelCount: 2, channelCountMode: 'explicit',
    outputChannelCount: [2], processorOptions: { wasmModule, config: configuration() },
  });
  const source = new ConstantSourceNode(context, { offset: 0.25 });
  source.connect(node).connect(context.destination);
  source.start();
  let failure = null;
  let welcomed = false;
  let sequence = 0;
  const outcomes = new Map();
  const records = [];
  node.port.onmessage = ({ data }) => {
    if (data.kind === 'failed' || data.kind === 'memory-growth') { failure = data; return; }
    if (data.kind !== 'records') return;
    const bytes = new Uint8Array(data.bytes);
    const view = new DataView(data.bytes);
    for (let at = 0; at < bytes.length;) {
      const length = view.getUint16(at, true);
      if (length < 3 || at + length > bytes.length) throw new Error('invalid engine framing');
      const message = bytes.slice(at + 2, at + length);
      records.push(message);
      if (message[0] === 0) {
        if (message[1] !== protocolVersion || message[2] !== 0) failure = 'protocol version mismatch';
        welcomed = true;
      }
      if (message[0] === 2) outcomes.set(Number(new DataView(message.buffer).getBigUint64(1, true)), message.slice(9));
      at += length;
    }
  };
  function send(message) {
    const length = message.length + 2;
    node.port.postMessage({ kind: 'records', bytes: Uint8Array.of(length & 255, length >>> 8, ...message).buffer });
  }
  const initial = context.suspend(0);
  const rendered = context.startRendering();
  await initial;
  async function tick() {
    if (failure) throw new Error(JSON.stringify(failure));
    const next = Math.round(context.currentTime * 8000) + 128;
    if (next >= context.length) throw new Error('acceptance exhausted its render window');
    const paused = context.suspend(next / 8000);
    await context.resume();
    await paused;
    await new Promise(resolve => setTimeout(resolve, 0));
  }
  send([0, protocolVersion, 0]);
  for (let n = 0; !welcomed && n < 100; n++) await tick();
  if (!welcomed) throw new Error('no Welcome');
  async function request(payload) {
    const correlation = ++sequence;
    send([1, ...u64(correlation), ...payload]);
    for (let n = 0; !outcomes.has(correlation) && n < 400; n++) await tick();
    const result = outcomes.get(correlation);
    if (!result || result[0] !== 0) throw new Error(`request ${payload[0]} failed: ${JSON.stringify(Array.from(result || []))}`);
  }
  const regions = [];
  async function region(name, expected) {
    for (let n = 0; n < 8; n++) await tick();
    const start = Math.round(context.currentTime * 8000);
    for (let n = 0; n < 2; n++) await tick();
    regions.push({ name, start, end: Math.round(context.currentTime * 8000), expected });
  }
  await request([0, ...id, 2, 2, 0, 0]);
  for (let channel = 0; channel < 2; channel++) {
    await request([10, 0, ...id, 0, channel, ...u32(channel), 1]);
    await request([11, ...id, 1, channel, ...id, channel, 1]);
  }
  await request([2, ...id, 2, 1]);
  await region('direct', 0.25);
  await request([70, ...id, 1]);
  await request([71, ...id, 0, ...u32(0x105), ...float(0)]);
  await request([71, ...id, 0, ...u32(0x101), ...float(1)]);
  await request([71, ...id, 0, ...u32(7), ...float(0)]);
  await request([71, ...id, 0, ...u32(8), ...float(-18)]);
  await region('processed', Math.fround(0.25 * Math.fround(10 ** (-18 / 20))));
  await request([71, ...id, 2, 0]);
  await region('bypassed', 0.25);
  await request([71, ...id, 2, 1]);
  await region('reactivated', Math.fround(0.25 * Math.fround(10 ** (-18 / 20))));
  await request([70, ...id, 0]);
  await region('removed', 0.25);
  source.stop(context.currentTime);
  node.port.postMessage({ kind: 'stop' });
  await context.resume();
  const output = await rendered;
  if (failure) throw new Error(JSON.stringify(failure));
  let samples = 0;
  for (const region of regions) {
    for (let channel = 0; channel < 2; channel++) {
      const data = output.getChannelData(channel);
      for (let frame = region.start; frame < region.end; frame++) {
        if (Math.abs(data[frame] - region.expected) > 1e-6) throw new Error(`${region.name} channel ${channel} frame ${frame}: ${data[frame]} != ${region.expected}`);
        samples++;
      }
    }
  }
  return { passed: true, context: 'actual OfflineAudioContext and AudioWorklet', crossOriginIsolated,
    samples, requests: sequence, records: records.length, regions, memoryGrowth: false };
}
