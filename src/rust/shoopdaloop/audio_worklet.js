import { ShoopRawWasmHost } from './raw_wasm_host.js';

class ShoopAudioProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.renderDiscontinuities = 0;
    this.callbackBudgetOverruns = 0;
    this.expectedFrame = null;
    try {
      const { wasmModule, maxQuantum, protocolVersion, commandMaxBytes } = options.processorOptions;
      this.protocolVersion = protocolVersion;
      this.host = new ShoopRawWasmHost(
        wasmModule,
        sampleRate,
        maxQuantum,
        commandMaxBytes,
      );
      this.port.onmessage = event => {
        try {
          if (event.data?.kind === 'shoop-trace-start') this.startTracing(event.data);
          else if (event.data?.kind === 'shoop-trace-stop') this.stopTracing(event.data);
          else this.handleCommand(event.data);
        } catch (error) {
          if (event.data?.kind === 'shoop-trace-start' && !this.trace) {
            this.reportTraceStartFailure(event.data);
          }
          this.fail(`AudioWorklet trace control failed: ${error?.stack || error}`);
        }
      };
    } catch (error) {
      this.initializationError = `AudioWorklet initialization failed: ${error?.stack || error}`;
    }
  }

  startTracing(options) {
    if (this.trace) throw new Error('AudioWorklet tracing is already active');
    const metadata = this.host.traceStart(
      options.realmId,
      options.clockId,
      options.capacityRecords,
      options.engineDetail,
    );
    const header = new Int32Array(options.sab, 0, 16);
    if (Atomics.load(header, 0) !== 0x50454631) {
      this.host.traceStop();
      throw new Error('AudioWorklet trace ring magic/version mismatch');
    }
    const capacity = Atomics.load(header, 1) >>> 0;
    if (capacity !== options.capacityRecords) {
      this.host.traceStop();
      throw new Error('AudioWorklet trace ring capacity mismatch');
    }
    Atomics.store(header, 11, currentFrame | 0);
    Atomics.store(header, 12, Math.floor(currentFrame / 0x1_0000_0000) | 0);
    this.host.traceSetFrame(currentFrame);
    this.trace = {
      header,
      data: new Uint8Array(options.sab, 64),
      capacity,
    };
    const timer = globalThis.performance;
    const hasReferenceClock = timer
      && Number.isFinite(timer.timeOrigin)
      && typeof timer.now === 'function';
    this.port.postMessage({
      kind: 'shoop-trace-metadata',
      metadata,
      sourceTicks: currentFrame,
      referenceMs: hasReferenceClock ? timer.timeOrigin + timer.now() : options.referenceMs,
      requestReferenceMs: options.referenceMs,
      fallbackClock: !hasReferenceClock,
    });
  }

  reportTraceStartFailure(options) {
    const timer = globalThis.performance;
    const hasReferenceClock = timer
      && Number.isFinite(timer.timeOrigin)
      && typeof timer.now === 'function';
    this.port.postMessage({
      kind: 'shoop-trace-stopped',
      dropped: 0,
      highWater: 0,
      sourceTicks: currentFrame,
      referenceMs: hasReferenceClock ? timer.timeOrigin + timer.now() : options.referenceMs,
      requestReferenceMs: options.referenceMs,
      fallbackClock: !hasReferenceClock,
      aborted: true,
    });
  }

  drainTracing() {
    if (!this.trace) return;
    const {header, data, capacity} = this.trace;
    let write = Atomics.load(header, 2) >>> 0;
    const read = Atomics.load(header, 3) >>> 0;
    let free = capacity - ((write - read) >>> 0);
    while (free > 0) {
      const slot = write % capacity;
      const records = Math.min(free, capacity - slot);
      const bytes = this.host.traceDrainInto(data, slot * 48, records * 48);
      if (!bytes) break;
      const written = bytes / 48;
      write = (write + written) >>> 0;
      free -= written;
    }
    const occupancy = (write - read) >>> 0;
    if (occupancy > (Atomics.load(header, 8) >>> 0)) Atomics.store(header, 8, occupancy);
    Atomics.store(header, 2, write | 0);
    Atomics.store(header, 4, this.host.traceDropped() | 0);
  }

  stopTracing(options = {}) {
    if (!this.trace) return;
    this.drainTracing();
    Atomics.store(this.trace.header, 6, 1);
    const timer = globalThis.performance;
    const hasReferenceClock = timer
      && Number.isFinite(timer.timeOrigin)
      && typeof timer.now === 'function';
    const status = {
      kind: 'shoop-trace-stopped',
      dropped: this.host.traceDropped(),
      highWater: Atomics.load(this.trace.header, 8) >>> 0,
      sourceTicks: currentFrame,
      referenceMs: hasReferenceClock ? timer.timeOrigin + timer.now() : options.referenceMs,
      requestReferenceMs: options.referenceMs,
      fallbackClock: !hasReferenceClock,
    };
    this.host.traceStop();
    this.trace = null;
    this.port.postMessage(status);
  }

  abortTracing() {
    if (!this.trace) return;
    const active = this.trace;
    try { this.drainTracing(); } catch (_) { /* preserve records already in the shared ring */ }
    Atomics.store(active.header, 6, 1);
    const timer = globalThis.performance;
    const hasReferenceClock = timer
      && Number.isFinite(timer.timeOrigin)
      && typeof timer.now === 'function';
    const status = {
      kind: 'shoop-trace-stopped',
      dropped: this.host?.traceDropped?.() || 0,
      highWater: Atomics.load(active.header, 8) >>> 0,
      sourceTicks: currentFrame,
      referenceMs: hasReferenceClock ? timer.timeOrigin + timer.now() : undefined,
      requestReferenceMs: undefined,
      fallbackClock: !hasReferenceClock,
      aborted: true,
    };
    try { this.host?.traceStop(); } catch (_) { /* host is already failing */ }
    this.trace = null;
    try { this.port.postMessage(status); } catch (_) { /* processor is terminating */ }
  }

  handleCommand(message) {
    try {
      let response = this.host.command(message);
      const event = JSON.parse(response);
      if (event.event?.kind === 'snapshot') {
        const diagnostics = this.host.diagnostics();
        event.event.render_discontinuities = this.renderDiscontinuities;
        event.event.callback_budget_overruns = this.callbackBudgetOverruns;
        event.event.memory_growths = diagnostics.memoryGrowths;
        event.event.render_memory_growths = diagnostics.renderMemoryGrowths;
        response = JSON.stringify(event);
      }
      this.port.postMessage(response);
      if (event.event?.kind === 'stopped') {
        this.stopped = true;
        this.abortTracing();
        this.host.destroy();
      }
    } catch (error) {
      this.fail(`AudioWorklet control command failed: ${error?.stack || error}`);
    }
  }

  fail(message) {
    if (!this.failureMessage) {
      this.failureMessage = message;
      this.port.postMessage(JSON.stringify({
        version: this.protocolVersion || 0,
        sequence: 0,
        event: { kind: 'error', message },
      }));
      this.abortTracing();
      this.host?.destroy();
    }
    return false;
  }

  process(inputs, outputs) {
    if (this.initializationError) return this.fail(this.initializationError);
    if (this.failureMessage || this.stopped) return false;
    const inputChannels = (inputs[0] || []).slice(0, 2);
    const outputChannels = (outputs[0] || []).slice(0, 2);
    const frames = outputChannels[0]?.length || inputChannels[0]?.length || 0;
    if (!frames) return this.fail('AudioWorklet supplied an empty render quantum');
    if (this.expectedFrame !== null && currentFrame !== this.expectedFrame) {
      this.renderDiscontinuities += 1;
    }
    this.expectedFrame = currentFrame + frames;
    if (this.trace) {
      Atomics.store(this.trace.header, 11, currentFrame | 0);
      Atomics.store(this.trace.header, 12, Math.floor(currentFrame / 0x1_0000_0000) | 0);
      Atomics.add(this.trace.header, 5, 1);
    }
    const timer = globalThis.performance;
    const startedAt = timer ? timer.now() : 0;
    try {
      this.host.traceSetFrame(currentFrame);
      this.host.process(inputChannels, outputChannels, frames);
      this.drainTracing();
    } catch (error) {
      return this.fail(`AudioWorklet process failed: ${error?.stack || error}`);
    }
    if (timer && timer.now() - startedAt > frames * 1000 / sampleRate) {
      this.callbackBudgetOverruns += 1;
    }
    return true;
  }
}

registerProcessor('shoop-audio-processor', ShoopAudioProcessor);
