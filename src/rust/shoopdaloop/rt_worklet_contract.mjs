// Contract of the new engine's AudioWorklet artifact and its processor script:
// the module has no imports, and the script prepares it from a configuration
// record and moves records, MIDI and audio over a simulated MessagePort.
import { readFile } from 'node:fs/promises';

const wasmPath = process.argv[2] || './dist/generated/shoop_rt_host_worklet.wasm';
const wasmModule = await WebAssembly.compile(await readFile(wasmPath));
const imports = WebAssembly.Module.imports(wasmModule);
if (imports.length !== 0) {
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
globalThis.sampleRate = 1000;
globalThis.currentFrame = 5000;
await import('./rt_audio_worklet.js');
if (registered?.name !== 'shoop-rt-audio-processor') {
  throw new Error('the processor script did not register its processor');
}

// The configuration record, written from its documented layout.
function configRecord({ version = 1, maxFrames = 128 } = {}) {
  const numbers = [
    version, 1000, maxFrames, 2, 2, 0, 8, 4, 0, 512, 256, 64, 512, 32, 8, 2, 0,
    16384, 16384, 4096, 4096, 8,
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
node.port.deliver({ kind: 'records', bytes: Uint8Array.of(1, 0).buffer });
const first = quantum(node, 0.5);
if (!first.alive) throw new Error('the processor stopped after its first quantum');
const replies = node.port.take('records');
if (replies.length !== 1 || !(replies[0].bytes instanceof ArrayBuffer)) {
  throw new Error(`expected one record batch, got ${JSON.stringify(replies)}`);
}
const welcome = new Uint8Array(replies[0].bytes);
if (welcome[2] !== 0 || welcome[3] !== 1 || welcome[4] !== 0) {
  throw new Error('the first engine record is not a Welcome for version 1');
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

node.port.deliver({ kind: 'stop' });
if (quantum(node, 0).alive) throw new Error('the processor kept running after stop');

for (const [config, stage, code] of [
  [configRecord({ version: 2 }), 'prepare', 3],
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
