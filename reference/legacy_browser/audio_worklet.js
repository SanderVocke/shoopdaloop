import { ShoopRawWasmHost, ShoopTraceChunkTransport } from './raw_wasm_host.js';

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
          else if (event.data?.kind === 'shoop-trace-abort') this.abortTracing();
          else if (event.data?.kind === 'shoop-trace-recycle') this.recycleTracing(event.data);
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
    this.host.traceSetFrame(currentFrame);
    this.trace = new ShoopTraceChunkTransport(this.host, this.port, options);
    const timer = globalThis.performance;
    const hasReferenceClock = timer
      && Number.isFinite(timer.timeOrigin)
      && typeof timer.now === 'function';
    this.port.postMessage({
      kind: 'shoop-trace-metadata',
      captureId: options.captureId,
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
      kind: 'shoop-trace-aborted',
      captureId: options.captureId,
      highWater: 0,
      sourceTicks: currentFrame,
      referenceMs: hasReferenceClock ? timer.timeOrigin + timer.now() : options.referenceMs,
      requestReferenceMs: options.referenceMs,
      fallbackClock: !hasReferenceClock,
      aborted: true,
    });
  }

  drainTracing() {
    this.trace?.drain();
  }

  recycleTracing(message) {
    if (!this.trace) return;
    this.trace.recycle(message);
    if (this.trace.finished) this.trace = null;
  }

  stopTracing(options = {}) {
    if (!this.trace || options.captureId !== this.trace.captureId) return;
    const timer = globalThis.performance;
    const hasReferenceClock = timer
      && Number.isFinite(timer.timeOrigin)
      && typeof timer.now === 'function';
    this.trace.stop({
      sourceTicks: currentFrame,
      referenceMs: hasReferenceClock ? timer.timeOrigin + timer.now() : options.referenceMs,
      requestReferenceMs: options.referenceMs,
      fallbackClock: !hasReferenceClock,
    });
    if (this.trace.finished) this.trace = null;
  }

  abortTracing() {
    if (!this.trace) return;
    const timer = globalThis.performance;
    const hasReferenceClock = timer
      && Number.isFinite(timer.timeOrigin)
      && typeof timer.now === 'function';
    this.trace.abort({
      sourceTicks: currentFrame,
      referenceMs: hasReferenceClock ? timer.timeOrigin + timer.now() : undefined,
      fallbackClock: !hasReferenceClock,
    });
    this.trace = null;
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
