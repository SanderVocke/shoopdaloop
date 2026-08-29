import { readFile } from 'node:fs/promises';
import { ShoopRawWasmHost } from './raw_wasm_host.js';

const wasmPath = process.argv[2] || './dist/generated/shoop_audio_worklet.wasm';
const bytes = await readFile(wasmPath);
const module = await WebAssembly.compile(bytes);
if (WebAssembly.Module.imports(module).length !== 0) {
  throw new Error('raw host contract requires an import-free Wasm artifact');
}
const protocolVersion = 18;
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
host.process([], [], 128);
host.exports.memory.grow(1);
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
