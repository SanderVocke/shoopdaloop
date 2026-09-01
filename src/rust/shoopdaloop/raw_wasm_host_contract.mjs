import { readFile } from 'node:fs/promises';
import { ShoopRawWasmHost } from './raw_wasm_host.js';

const wasmPath = process.argv[2] || './dist/generated/shoop_audio_worklet.wasm';
const bytes = await readFile(wasmPath);
const module = await WebAssembly.compile(bytes);
if (WebAssembly.Module.imports(module).length !== 0) {
  throw new Error('raw host contract requires an import-free Wasm artifact');
}
const protocolVersion = 19;
const host = new ShoopRawWasmHost(module, 48000, 2048, 262144);
const poll = JSON.stringify({ version: protocolVersion, sequence: 1, command: { kind: 'poll' } });
const first = JSON.parse(host.command(poll));
if (first.version !== protocolVersion || first.sequence !== 1 || first.event?.kind !== 'snapshot') {
  throw new Error(`unexpected raw host response: ${JSON.stringify(first)}`);
}
const malformed = JSON.parse(host.command('{'));
if (malformed.event?.kind !== 'error') throw new Error('malformed protocol bytes were accepted');
let capacityRejected = false;
try {
  host.command('x'.repeat(262145));
} catch (error) {
  capacityRejected = error instanceof RangeError;
}
if (!capacityRejected) throw new Error('oversized raw host command was not rejected');
const metadata = host.traceStart(4, 104, 1024, true);
if (!metadata.some(entry => entry.label === 'engine.rt.callback')) {
  throw new Error('raw trace metadata omitted the callback span');
}
host.traceSetFrame(256);
host.process([], [], 128);
const traceBytes = new Uint8Array(1024 * 48);
const wrapWrite = 1023;
const wrappedRecords = host.traceDrainRing(traceBytes, wrapWrite, 1024);
if (wrappedRecords <= 1) throw new Error(`trace group did not cross ring boundary: ${wrappedRecords}`);
const traceLength = wrappedRecords * 48;
const firstTraceRecord = new DataView(traceBytes.buffer, wrapWrite * 48, 48);
if (firstTraceRecord.getUint32(4, true) !== 4 || firstTraceRecord.getUint32(12, true) !== 104) {
  throw new Error('raw trace record lost realm or clock identity');
}
if (firstTraceRecord.getUint32(16, true) !== 256 || firstTraceRecord.getUint32(20, true) !== 0) {
  throw new Error('raw trace record lost exact source frame');
}
if (!traceBytes[0]) throw new Error('wrapped trace group was not copied into the ring head');
host.traceSetFrame(320);
host.process([], [], 128);
if (!host.traceDrainRing(traceBytes, wrapWrite + wrappedRecords, 1024 - wrappedRecords)) {
  throw new Error('trace producer stalled after wrapping');
}
host.exports.memory.grow(1);
host.traceSetFrame(384);
host.process([], [], 128);
traceBytes.fill(0);
const postGrowthTraceLength = host.traceDrainInto(traceBytes);
if (!postGrowthTraceLength || traceBytes[0] !== 1) {
  throw new Error('raw trace views did not recover after Wasm memory growth');
}
host.traceStop();
host.command(JSON.stringify({ version: protocolVersion, sequence: 2, command: { kind: 'poll' } }));
if (host.diagnostics().memoryGrowths < 1) throw new Error('memory growth was not diagnosed');
host.destroy();
host.destroy();
let shutdownRejected = false;
try {
  host.process([], [], 128);
} catch (_error) {
  shutdownRejected = true;
}
if (!shutdownRejected) throw new Error('destroyed raw host accepted processing');
const trapped = new ShoopRawWasmHost(module, 48000, 128, 262144);
trapped.host = 0xfffffff0;
let trapObserved = false;
try {
  trapped.process([], [], 128);
} catch (error) {
  trapObserved = error instanceof WebAssembly.RuntimeError;
}
if (!trapObserved) throw new Error('Wasm ABI trap was not observable through the raw host');
console.log('raw Wasm host artifact contract: ok');
