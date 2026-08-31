import { ShoopRawWasmHost } from './raw_wasm_host.js';

let applicationPort = null;
let fixturePort = null;
let protocolVersion = 0;
let host = null;
let scheduler = null;
let terminal = false;
let trace = null;

function releaseAndClose(notifyStopped) {
  scheduler?.stop();
  abortTracing();
  host?.destroy();
  if (notifyStopped) fixturePort?.postMessage({ kind: 'stopped' });
  applicationPort?.close();
  fixturePort?.close();
  self.close();
}

function applicationFailure(message) {
  if (terminal) return;
  terminal = true;
  scheduler?.stop();
  applicationPort?.postMessage(JSON.stringify({
    version: protocolVersion,
    sequence: 0,
    event: { kind: 'error', message },
  }));
  fixturePort?.postMessage({ kind: 'failure', message });
  setTimeout(() => releaseAndClose(false), 0);
}

function startTracing(options) {
  if (trace) throw new Error('Worker tracing is already active');
  const metadata = host.traceStart(
    options.realmId,
    options.clockId,
    options.capacityRecords,
    options.engineDetail,
  );
  const header = new Int32Array(options.sab, 0, 16);
  if (Atomics.load(header, 0) !== 0x50454631) {
    host.traceStop();
    throw new Error('Worker trace ring magic/version mismatch');
  }
  const capacity = Atomics.load(header, 1) >>> 0;
  const frame = scheduler.processedQuanta * scheduler.quantum;
  const referenceMs = Math.max(0, Math.round(options.referenceMs));
  Atomics.store(header, 9, frame | 0);
  Atomics.store(header, 10, Math.floor(frame / 0x1_0000_0000) | 0);
  Atomics.store(header, 13, referenceMs | 0);
  Atomics.store(header, 14, Math.floor(referenceMs / 0x1_0000_0000) | 0);
  Atomics.store(header, 15, 1);
  Atomics.store(header, 11, frame | 0);
  Atomics.store(header, 12, Math.floor(frame / 0x1_0000_0000) | 0);
  host.traceSetFrame(frame);
  trace = {header, data: new Uint8Array(options.sab, 64), capacity};
  applicationPort.postMessage({
    kind: 'shoop-trace-metadata',
    metadata,
    sourceTicks: frame,
    referenceMs: performance.timeOrigin + performance.now(),
  });
}

function drainTracing() {
  if (!trace) return;
  let write = Atomics.load(trace.header, 2) >>> 0;
  const read = Atomics.load(trace.header, 3) >>> 0;
  let free = trace.capacity - ((write - read) >>> 0);
  while (free > 0) {
    const written = host.traceDrainRing(trace.data, write, free);
    if (!written) break;
    write = (write + written) >>> 0;
    free -= written;
  }
  const occupancy = (write - read) >>> 0;
  if (occupancy > (Atomics.load(trace.header, 8) >>> 0)) Atomics.store(trace.header, 8, occupancy);
  Atomics.store(trace.header, 2, write | 0);
  Atomics.store(trace.header, 4, host.traceDropped() | 0);
}

function stopTracing() {
  if (!trace) return;
  drainTracing();
  Atomics.store(trace.header, 6, 1);
  const status = {
    kind: 'shoop-trace-stopped',
    dropped: host.traceDropped(),
    highWater: Atomics.load(trace.header, 8) >>> 0,
    sourceTicks: scheduler.processedQuanta * scheduler.quantum,
    referenceMs: performance.timeOrigin + performance.now(),
  };
  host.traceStop();
  trace = null;
  applicationPort.postMessage(status);
}

function abortTracing() {
  if (!trace) return;
  const active = trace;
  try { drainTracing(); } catch (_) { /* preserve records already in the shared ring */ }
  Atomics.store(active.header, 6, 1);
  const status = {
    kind: 'shoop-trace-stopped',
    dropped: host?.traceDropped?.() || 0,
    highWater: Atomics.load(active.header, 8) >>> 0,
    sourceTicks: (scheduler?.processedQuanta || 0) * (scheduler?.quantum || 0),
    referenceMs: performance.timeOrigin + performance.now(),
    aborted: true,
  };
  try { host?.traceStop(); } catch (_) { /* host is already failing */ }
  trace = null;
  try { applicationPort?.postMessage(status); } catch (_) { /* realm is terminating */ }
}

function handleApplicationCommand(message) {
  if (terminal) return;
  try {
    if (message?.kind === 'shoop-trace-start') {
      startTracing(message);
      return;
    }
    if (message?.kind === 'shoop-trace-stop') {
      stopTracing();
      return;
    }
    let response = host.command(message);
    const event = JSON.parse(response);
    if (event.event?.kind === 'snapshot') {
      const bridge = host.diagnostics();
      const timing = scheduler.diagnostics();
      event.event.render_discontinuities = timing.discontinuities;
      event.event.callback_budget_overruns = timing.overruns;
      event.event.memory_growths = bridge.memoryGrowths;
      event.event.render_memory_growths = bridge.renderMemoryGrowths;
      response = JSON.stringify(event);
    }
    applicationPort.postMessage(response);
    if (event.event?.kind === 'stopped') {
      terminal = true;
      setTimeout(() => releaseAndClose(true), 0);
    }
  } catch (error) {
    if (message?.kind === 'shoop-trace-start' && !trace) {
      const frame = (scheduler?.processedQuanta || 0) * (scheduler?.quantum || 0);
      try {
        applicationPort?.postMessage({
          kind: 'shoop-trace-stopped',
          dropped: 0,
          highWater: 0,
          sourceTicks: frame,
          referenceMs: performance.timeOrigin + performance.now(),
          aborted: true,
        });
      } catch (_) { /* realm is terminating */ }
    }
    applicationFailure(`Worker engine command failed: ${error?.stack || error}`);
  }
}

class WorkerScheduler {
  constructor(rawHost, mode, sampleRate, quantum) {
    this.host = rawHost;
    this.mode = mode;
    this.sampleRate = sampleRate;
    this.quantum = quantum;
    this.timer = null;
    this.running = false;
    this.frameRemainder = 0;
    this.lastTick = 0;
    this.discontinuities = 0;
    this.overruns = 0;
    this.processedQuanta = 0;
  }

  start() {
    if (this.running || this.mode === 'explicit') return;
    this.running = true;
    this.lastTick = performance.now();
    this.schedule();
  }

  schedule() {
    if (!this.running || terminal) return;
    this.timer = setTimeout(() => this.tick(), this.mode === 'realtime' ? 4 : 0);
  }

  processOne(inputs = [], outputs = []) {
    const startedAt = performance.now();
    const frame = this.processedQuanta * this.quantum;
    this.host.traceSetFrame(frame);
    this.host.process(inputs, outputs, this.quantum);
    this.processedQuanta += 1;
    if (trace) {
      Atomics.store(trace.header, 11, frame | 0);
      Atomics.store(trace.header, 12, Math.floor(frame / 0x1_0000_0000) | 0);
      Atomics.add(trace.header, 5, 1);
      drainTracing();
    }
    if (performance.now() - startedAt > this.quantum * 1000 / this.sampleRate) {
      this.overruns += 1;
    }
  }

  tick() {
    this.timer = null;
    if (!this.running || terminal) return;
    try {
      if (this.mode === 'cooperative') {
        for (let index = 0; index < 8; index += 1) this.processOne();
      } else {
        const now = performance.now();
        const rawElapsed = Math.max(0, (now - this.lastTick) / 1000);
        this.lastTick = now;
        const elapsed = Math.min(rawElapsed, 0.1);
        if (rawElapsed > elapsed) this.discontinuities += 1;
        this.frameRemainder += elapsed * this.sampleRate;
        let processed = 0;
        while (this.frameRemainder >= this.quantum && processed < 32) {
          this.processOne();
          this.frameRemainder -= this.quantum;
          processed += 1;
        }
        if (this.frameRemainder >= this.quantum) {
          this.discontinuities += 1;
          this.frameRemainder %= this.quantum;
        }
      }
      this.schedule();
    } catch (error) {
      applicationFailure(`Worker engine processing failed: ${error?.stack || error}`);
    }
  }

  pause() {
    this.running = false;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  resume() {
    this.start();
  }

  stop() {
    this.pause();
    this.frameRemainder = 0;
  }

  diagnostics() {
    return {
      mode: this.mode,
      running: this.running,
      discontinuities: this.discontinuities,
      overruns: this.overruns,
      processedQuanta: this.processedQuanta,
    };
  }
}

function handleFixtureCommand(message) {
  if (!fixturePort || terminal || !message || typeof message !== 'object') return;
  try {
    switch (message.kind) {
      case 'process': {
        if (scheduler.mode !== 'explicit') throw new Error('explicit process requires explicit mode');
        const frames = message.frames || scheduler.quantum;
        if (frames !== scheduler.quantum) throw new Error('fixture process must use configured quantum');
        const inputs = (message.inputs || []).map(values => Float32Array.from(values));
        const outputs = Array.from(
          { length: message.outputChannels || 0 },
          () => new Float32Array(frames),
        );
        scheduler.processOne(inputs, outputs);
        fixturePort.postMessage({
          kind: 'processed',
          outputs: outputs.map(channel => Array.from(channel)),
          diagnostics: scheduler.diagnostics(),
        });
        break;
      }
      case 'pause':
        scheduler.pause();
        fixturePort.postMessage({ kind: 'state', diagnostics: scheduler.diagnostics() });
        break;
      case 'resume':
        scheduler.resume();
        fixturePort.postMessage({ kind: 'state', diagnostics: scheduler.diagnostics() });
        break;
      case 'state':
        fixturePort.postMessage({ kind: 'state', diagnostics: scheduler.diagnostics() });
        break;
      case 'shutdown':
        shutdown();
        break;
      default:
        throw new Error(`unknown fixture command ${message.kind}`);
    }
  } catch (error) {
    fixturePort.postMessage({ kind: 'fixture-error', message: `${error?.stack || error}` });
  }
}

function shutdown() {
  if (terminal) return;
  terminal = true;
  releaseAndClose(true);
}

self.onmessage = event => {
  if (host) return;
  try {
    const options = event.data;
    if (!options || options.kind !== 'initialize' || !options.applicationPort) {
      throw new Error('invalid Worker engine bootstrap');
    }
    protocolVersion = options.protocolVersion;
    applicationPort = options.applicationPort;
    fixturePort = options.fixturePort || null;
    host = new ShoopRawWasmHost(
      options.wasmModule,
      options.sampleRate,
      options.maxQuantum,
      options.commandMaxBytes,
    );
    scheduler = new WorkerScheduler(
      host,
      options.processingMode,
      options.sampleRate,
      options.quantum,
    );
    applicationPort.onmessage = command => handleApplicationCommand(command.data);
    applicationPort.start();
    if (fixturePort) {
      fixturePort.onmessage = command => handleFixtureCommand(command.data);
      fixturePort.start();
      fixturePort.postMessage({ kind: 'ready', diagnostics: scheduler.diagnostics() });
    }
    scheduler.start();
  } catch (error) {
    applicationFailure(`Worker engine initialization failed: ${error?.stack || error}`);
  }
};
