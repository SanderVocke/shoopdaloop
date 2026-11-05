// Contract of the new engine's AudioWorklet artifact and its processor script:
// the module imports only its profiling clock, and accepts a configuration
// record and moves records, MIDI and audio over a simulated MessagePort.
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';

const wasmPath = process.argv[2] || './dist/generated/shoop_rt_host_worklet.wasm';
const codecPath = process.argv[3] || join(dirname(wasmPath), 'shoop_persistence_codec_bg.wasm');
const codecModule = await WebAssembly.compile(await readFile(codecPath));
const codecBindingsPath = join(dirname(codecPath), 'shoop_persistence_codec.js');
const codecBindings = await readFile(codecBindingsPath);
const { default: initializeCodec } = await import(`data:text/javascript;base64,${codecBindings.toString('base64')}`);
const codec = { exports: await initializeCodec({ module_or_path: codecModule }) };
for (const name of ['memory', 'codec_input', 'codec_run', 'codec_output']) {
  if (!(name in codec.exports)) throw new Error(`the client codec worker Wasm lacks ${name}`);
}
console.log('client codec worker artifact contract: ok');

const wasmModule = await WebAssembly.compile(await readFile(wasmPath));
const imports = WebAssembly.Module.imports(wasmModule);
if (imports.length !== 1 || imports[0].module !== "shoop_profiling" || imports[0].name !== "now" || imports[0].kind !== "function") {
  throw new Error(`the new engine worklet Wasm has imports: ${JSON.stringify(imports)}`);
}
const exported = new Set(WebAssembly.Module.exports(wasmModule).map(entry => entry.name));
for (const name of [
  'memory',
  'shoop_rt_worklet_config_buffer',
  'shoop_rt_worklet_prepare',
  'shoop_rt_worklet_release',
  'shoop_rt_worklet_max_frames',
  'shoop_rt_worklet_audio_inputs',
  'shoop_rt_worklet_audio_outputs',
  'shoop_rt_worklet_audio_input_ptr',
  'shoop_rt_worklet_audio_output_ptr',
  'shoop_rt_worklet_buffer_ptr',
  'shoop_rt_worklet_buffer_capacity',
  'shoop_rt_worklet_buffer_len',
  'shoop_rt_worklet_commit',
  'shoop_rt_worklet_process',
  'shoop_rt_worklet_counter',
  'shoop_rt_worklet_profiling_clock',
  'shoop_rt_worklet_profiling_active',
  'shoop_rt_worklet_profiling_credit',
  'shoop_rt_worklet_profiling_begin',
  'shoop_rt_worklet_profiling_end',
]) {
  if (!exported.has(name)) throw new Error(`the new engine worklet Wasm lacks ${name}`);
}

class SimulatedPort {
  constructor() {
    this.onmessage = null;
    this.sent = [];
  }
  postMessage(data) {
    this.sent.push(data);
  }
  deliver(data) {
    this.onmessage?.({ data });
  }
  take(kind) {
    const taken = this.sent.filter(message => message.kind === kind);
    this.sent = this.sent.filter(message => message.kind !== kind);
    return taken;
  }
}

let registered = null;
globalThis.AudioWorkletProcessor = class {
  constructor() {
    this.port = new SimulatedPort();
  }
};
globalThis.registerProcessor = (name, processor) => {
  registered = { name, processor };
};
let clockCalls = 0;
let clockMillis = 0;
Object.defineProperty(globalThis, 'performance', {
  configurable: true,
  value: { now() { clockCalls += 1; clockMillis += 0.001; return clockMillis; } },
});
globalThis.sampleRate = 1000;
globalThis.currentFrame = 5000;
await import('./rt_audio_worklet.js');
if (registered?.name !== 'shoop-rt-audio-processor') {
  throw new Error('the processor script did not register its processor');
}

// The configuration record, written from its documented layout.
function configRecord({ version = 4, maxFrames = 128 } = {}) {
  const numbers = [
    version, 1000, maxFrames, 1, 2, 2, 8, 8, 0, 8, 4, 0, 512, 256, 64, 512, 32, 8, 2, 0,
    16384, 16384, 4096, 4096, 8, 256, 1024, 512,
    1, 2, 4,
    2, 0, 0, 1, 1,
  ];
  const portNames = [['capture_1', 'capture_2'], ['destination_1', 'destination_2'], ['keys'], []];
  const bytes = [];
  const u32 = value => bytes.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, value >>> 24);
  numbers.forEach(u32);
  for (const names of portNames) {
    u32(names.length);
    for (const name of names) bytes.push(name.length, ...Array.from(name, c => c.charCodeAt(0)));
  }
  return new Uint8Array(bytes).buffer;
}

function processor(config) {
  return new registered.processor({ processorOptions: { wasmModule, config } });
}

function quantum(node, input) {
  const inputs = [[new Float32Array(128).fill(input), new Float32Array(128).fill(input)]];
  const outputs = [[new Float32Array(128).fill(9), new Float32Array(128).fill(9)]];
  const alive = node.process(inputs, outputs);
  globalThis.currentFrame += 128;
  return { alive, outputs: outputs[0] };
}

const node = processor(configRecord());
if (node.port.take('failed').length) throw new Error('a valid configuration was refused');
const preparedBytes = node.wasm.memory.buffer.byteLength;

// A Hello record split across two messages.
node.port.deliver({ kind: 'records', bytes: Uint8Array.of(5, 0, 0) });
const engineProtocolVersion = 11;
node.port.deliver({ kind: 'records', bytes: Uint8Array.of(engineProtocolVersion, 0).buffer });
const disabledClockCalls = clockCalls;
const first = quantum(node, 0.5);
if (clockCalls - disabledClockCalls !== 2) throw new Error('disabled profiling read an extra clock');
if (!first.alive) throw new Error('the processor stopped after its first quantum');
const replies = node.port.take('records');
if (replies.length !== 1 || !(replies[0].bytes instanceof ArrayBuffer)) {
  throw new Error(`expected one record batch, got ${JSON.stringify(replies)}`);
}
const welcome = new Uint8Array(replies[0].bytes);
if (welcome[2] !== 0 || welcome[3] !== engineProtocolVersion || welcome[4] !== 0) {
  throw new Error(`the first engine record is not a Welcome for version ${engineProtocolVersion}`);
}
if (first.outputs.some(channel => channel.some(sample => sample !== 0))) {
  throw new Error('an engine without strips rendered non-silent output');
}

// An input without connected channels, as before the microphone is connected.
{
  const outputs = [[new Float32Array(128).fill(9), new Float32Array(128).fill(9)]];
  if (!node.process([[]], outputs)) throw new Error('the processor stopped without input channels');
  globalThis.currentFrame += 128;
  if (outputs[0].some(channel => channel.some(sample => sample !== 0))) {
    throw new Error('an engine without input channels rendered non-silent output');
  }
}

// A MIDI event for a port that does not exist, then a late one on the only port.
const event = (frame, port, message) => {
  const bytes = new Uint8Array(10 + message.length);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(frame), true);
  bytes[8] = port;
  bytes[9] = message.length;
  bytes.set(message, 10);
  return bytes;
};
node.port.deliver({ kind: 'midi', bytes: event(globalThis.currentFrame, 3, [0x90, 60, 100]) });
node.port.deliver({ kind: 'midi', bytes: event(globalThis.currentFrame - 10, 0, [0x90, 61, 100]) });
for (let index = 0; index < 200; index += 1) quantum(node, 0.25);
node.port.deliver({ kind: 'status' });
const [status] = node.port.take('status');
const counters = status?.counters;
if (counters?.midiInDropped !== 1 || counters?.midiInLate !== 1 || counters?.discontinuities !== 0) {
  throw new Error(`unexpected counters: ${JSON.stringify(counters)}`);
}
if (node.wasm.memory.buffer.byteLength !== preparedBytes) {
  throw new Error('module memory grew after preparation');
}
if (!node.port.take('records').length) throw new Error('the connected engine sent no telemetry');

// The runtime profiling request uses its appended protocol tag.
function profilingRequest(enabled, correlation) {
  const bytes = new Uint8Array(13);
  const view = new DataView(bytes.buffer);
  view.setUint16(0, bytes.length, true);
  bytes[2] = 1;
  view.setBigUint64(3, BigInt(correlation), true);
  bytes[11] = 40;
  bytes[12] = enabled ? 1 : 0;
  node.port.deliver({ kind: 'records', bytes });
}
function takeEngineRecords(returnCredits = true) {
  const records = [];
  for (const { bytes } of node.port.take('records')) {
    const view = new DataView(bytes);
    for (let offset = 0; offset < bytes.byteLength;) {
      const length = view.getUint16(offset, true);
      if (length < 3 || offset + length > bytes.byteLength) throw new Error('invalid profiling record batch');
      records.push(new Uint8Array(bytes, offset, length));
      offset += length;
    }
  }
  if (returnCredits) {
    const count = records.filter(record => record[2] >= 8 && record[2] <= 10).length;
    if (count) node.port.deliver({ kind: 'profiling-credit', count });
  }
  return records;
}
// Create a stereo/MIDI strip so the fixture has compiled nodes to measure.
{
  const bytes = new Uint8Array(24);
  const view = new DataView(bytes.buffer);
  view.setUint16(0, bytes.length, true);
  bytes[2] = 1;
  view.setBigUint64(3, 990n, true);
  bytes[11] = 0;
  view.setUint32(12, 0, true);
  view.setUint32(16, 1, true);
  bytes.set([2, 2, 1, 1], 20);
  node.port.deliver({ kind: 'records', bytes });
  for (let index = 0; index < 32; index += 1) quantum(node, 0.25);
  takeEngineRecords();
}
profilingRequest(true, 991);
const enabledClockCalls = clockCalls;
quantum(node, 0.25);
if (clockCalls - enabledClockCalls <= 2 || !node.wasm.shoop_rt_worklet_profiling_active()) {
  throw new Error('runtime profiling did not use the host elapsed-time clock');
}
let timedNodes = 0;
const profileRecords = takeEngineRecords();
if (!profileRecords.some(record => record[2] === 2 &&
    new DataView(record.buffer, record.byteOffset, record.byteLength).getBigUint64(3, true) === 991n && record[11] === 0)) {
  throw new Error('profiling request did not receive a successful completion');
}
for (const record of profileRecords) {
  if (record[2] !== 9) continue;
  const view = new DataView(record.buffer, record.byteOffset, record.byteLength);
  const start = view.getBigUint64(59, true);
  const end = view.getBigUint64(67, true);
  for (let index = 0; index < record[87]; index += 1) {
    const key = view.getBigUint64(96 + index * 16, true);
    const at = view.getBigUint64(104 + index * 16, true);
    if (at < start || at > end) throw new Error('node timing escaped its measured interval');
    if ((key & ((1n << 63n) | (1n << 62n))) === 0n) timedNodes += 1;
  }
}
if (!timedNodes) throw new Error('profiling sent no graph node completion timestamps');
node.load = 2;
const captureMessages = [];
for (let index = 0; index < 64; index += 1) {
  quantum(node, 0.25);
  captureMessages.push(...takeEngineRecords());
}
const captureRecords = captureMessages.filter(record => record[2] === 9).map(record =>
  new DataView(record.buffer, record.byteOffset, record.byteLength));
if (!captureRecords.some(view => view.getBigUint64(11, true) !== 0n && (view.getUint32(88, true) & 4))) {
  throw new Error('a worklet overrun did not produce a complete bounded xrun capture');
}
let stalledRecords = 0;
for (let index = 0; index < 512; index += 1) {
  quantum(node, 0.25);
  stalledRecords += takeEngineRecords(false).filter(record => record[2] >= 8 && record[2] <= 10).length;
}
if (!stalledRecords || stalledRecords > 256) throw new Error('profiling exceeded its browser credit window');
profilingRequest(false, 992);
quantum(node, 0.25);
if (!takeEngineRecords(false).some(record => record[2] === 2 &&
    new DataView(record.buffer, record.byteOffset, record.byteLength).getBigUint64(3, true) === 992n && record[11] === 0)) {
  throw new Error('stalled profiling blocked a reliable runtime control');
}
node.port.deliver({ kind: 'profiling-credit', count: stalledRecords });
const stoppedClockCalls = clockCalls;
quantum(node, 0.25);
if (clockCalls - stoppedClockCalls !== 2 || node.wasm.shoop_rt_worklet_profiling_active()) {
  throw new Error('runtime profiling did not stop reading its clock');
}
if (node.wasm.memory.buffer.byteLength !== preparedBytes) throw new Error('profiling grew worklet memory after preparation');
console.log('worklet profiling clocks, timestamps, xrun capture and runtime toggle: ok');

node.port.deliver({ kind: 'stop' });
if (quantum(node, 0).alive) throw new Error('the processor kept running after stop');

for (const [config, stage, code] of [
  [configRecord({ version: 1 }), 'prepare', 3],
  [configRecord({ maxFrames: 0 }), 'prepare', 4],
  [new ArrayBuffer(0), 'instantiate', 0],
]) {
  const refused = processor(config);
  const [failure] = refused.port.take('failed');
  if (failure?.stage !== stage || failure?.code !== code) {
    throw new Error(`expected a ${stage} failure ${code}, got ${JSON.stringify(failure)}`);
  }
  if (quantum(refused, 0).alive) throw new Error('a refused processor kept running');
}
console.log('new engine AudioWorklet artifact contract: ok');
