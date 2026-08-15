import { ShoopRawWasmHost } from './raw_wasm_host.js';

let applicationPort = null;
let fixturePort = null;
let protocolVersion = 0;
let host = null;
let scheduler = null;
let terminal = false;

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
  host?.destroy();
}

function handleApplicationCommand(message) {
  if (terminal) return;
  try {
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
  } catch (error) {
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
    this.host.process(inputs, outputs, this.quantum);
    this.processedQuanta += 1;
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
  scheduler?.stop();
  host?.destroy();
  applicationPort?.close();
  fixturePort?.postMessage({ kind: 'stopped' });
  fixturePort?.close();
  self.close();
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
