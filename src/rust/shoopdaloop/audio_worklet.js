import './raw_wasm_host.js';

class ShoopAudioProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.renderDiscontinuities = 0;
    this.callbackBudgetOverruns = 0;
    this.expectedFrame = null;
    try {
      const { wasmModule, maxQuantum, protocolVersion, commandMaxBytes } = options.processorOptions;
      this.protocolVersion = protocolVersion;
      this.host = new globalThis.ShoopRawWasmHost(
        wasmModule,
        sampleRate,
        maxQuantum,
        commandMaxBytes,
      );
      this.port.onmessage = event => this.handleCommand(event.data);
    } catch (error) {
      this.initializationError = `AudioWorklet initialization failed: ${error?.stack || error}`;
    }
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
      this.host?.destroy();
    }
    return false;
  }

  process(inputs, outputs) {
    if (this.initializationError) return this.fail(this.initializationError);
    if (this.failureMessage) return false;
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
      this.host.process(inputChannels, outputChannels, frames);
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
