import fs from 'node:fs/promises';
import { MessageChannel, Worker } from 'node:worker_threads';
import { pathToFileURL } from 'node:url';

const [wasmPath, workerPath, bootstrapPath, configPath] = process.argv.slice(2);
if (!wasmPath || !workerPath || !bootstrapPath || !configPath) {
  throw new Error('expected worklet Wasm, Worker module, Node bootstrap, and protocol config');
}
const wasmModule = await WebAssembly.compile(await fs.readFile(wasmPath));
const config = JSON.parse(await fs.readFile(configPath, 'utf8'));

function nextMessage(target, timeout = 5000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Worker probe message timed out')), timeout);
    target.once('message', message => {
      clearTimeout(timer);
      resolve(message);
    });
  });
}

async function spawn() {
  const worker = new Worker(pathToFileURL(bootstrapPath), {
    type: 'module',
    workerData: { workerModuleUrl: pathToFileURL(workerPath).href },
  });
  const bootstrap = await nextMessage(worker);
  if (bootstrap.kind !== 'node-bootstrap-ready') throw new Error('Node bootstrap failed');
  const application = new MessageChannel();
  const fixture = new MessageChannel();
  worker.postMessage({
    kind: 'initialize',
    wasmModule,
    applicationPort: application.port2,
    fixturePort: fixture.port2,
    sampleRate: 48000,
    quantum: 128,
    maxQuantum: 2048,
    protocolVersion: config.protocol_version,
    commandMaxBytes: config.command_max_bytes,
    processingMode: 'explicit',
  }, [application.port2, fixture.port2]);
  const ready = await nextMessage(fixture.port1);
  if (ready.kind !== 'ready') throw new Error(`engine not ready: ${JSON.stringify(ready)}`);
  return { worker, application: application.port1, fixture: fixture.port1 };
}

async function poll(instance, sequence) {
  instance.application.postMessage(JSON.stringify({
    version: config.protocol_version,
    sequence,
    command: { kind: 'poll' },
  }));
  const response = JSON.parse(await nextMessage(instance.application));
  if (response.sequence !== sequence || response.event?.kind !== 'snapshot') {
    throw new Error(`poll mismatch: ${JSON.stringify(response)}`);
  }
  return response.event;
}

async function fixtureCommand(instance, command) {
  instance.fixture.postMessage(command);
  return nextMessage(instance.fixture);
}

async function stop(instance) {
  const exit = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Worker exit timed out')), 5000);
    instance.worker.once('exit', code => {
      clearTimeout(timer);
      resolve(code);
    });
  });
  const stopped = await fixtureCommand(instance, { kind: 'shutdown' });
  if (stopped.kind !== 'stopped') throw new Error('shutdown acknowledgement mismatch');
  instance.application.close();
  instance.fixture.close();
  const code = await exit;
  if (code !== 0) throw new Error(`Worker exited with ${code}`);
}

const first = await spawn();
const second = await spawn();
const [firstInitial, secondInitial] = await Promise.all([poll(first, 1), poll(second, 1)]);
if (firstInitial.callback_count !== 0 || secondInitial.callback_count !== 0) {
  throw new Error('fresh Worker instances shared progression');
}
const processed = await fixtureCommand(first, {
  kind: 'process',
  frames: 128,
  inputs: [[0.25, ...Array(127).fill(0)]],
  outputChannels: 2,
});
if (processed.kind !== 'processed'
    || processed.outputs.length !== 2
    || processed.diagnostics.processedQuanta !== 1) {
  throw new Error(`explicit processing mismatch: ${JSON.stringify(processed)}`);
}
const [advanced, isolated] = await Promise.all([poll(first, 2), poll(second, 2)]);
if (advanced.callback_count !== 1 || isolated.callback_count !== 0) {
  throw new Error('Worker instances leaked engine state');
}
await Promise.all([stop(first), stop(second)]);
console.log('node Worker production-module probe: ok');
