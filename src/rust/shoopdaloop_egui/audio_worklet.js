const MAX_CHANNELS = 2;
const MAX_COMMAND_BYTES = 16 * 1024;
const PROTOCOL_VERSION = 4;

function encodeUtf8(value) {
  const bytes = [];
  for (const character of value) {
    const code = character.codePointAt(0);
    if (code < 0x80) bytes.push(code);
    else if (code < 0x800) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    else if (code < 0x10000) {
      bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    } else {
      bytes.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 0x3f), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    }
  }
  return new Uint8Array(bytes);
}

function decodeUtf8(bytes) {
  let output = '';
  for (let index = 0; index < bytes.length;) {
    const first = bytes[index++];
    let code;
    if (first < 0x80) code = first;
    else if (first < 0xe0) code = ((first & 0x1f) << 6) | (bytes[index++] & 0x3f);
    else if (first < 0xf0) code = ((first & 0x0f) << 12) | ((bytes[index++] & 0x3f) << 6) | (bytes[index++] & 0x3f);
    else {
      code = ((first & 7) << 18) | ((bytes[index++] & 0x3f) << 12) | ((bytes[index++] & 0x3f) << 6) | (bytes[index++] & 0x3f);
    }
    output += String.fromCodePoint(code);
  }
  return output;
}

class ShoopAudioProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.renderDiscontinuities = 0;
    this.callbackBudgetOverruns = 0;
    this.memoryGrowths = 0;
    this.expectedFrame = null;
    try {
      const { wasmModule, maxQuantum } = options.processorOptions;
      this.maxQuantum = maxQuantum;
      this.instance = new WebAssembly.Instance(wasmModule, {});
      this.exports = this.instance.exports;
      this.host = this.exports.shoop_worklet_create(sampleRate, maxQuantum);
      if (!this.host) throw new Error('could not create the Shoop worklet host');
      this.memoryBuffer = null;
      this.refreshViews();
      this.port.onmessage = event => this.handleCommand(event.data);
    } catch (error) {
      this.initializationError = `AudioWorklet initialization failed: ${error?.stack || error}`;
    }
  }

  refreshViews() {
    const memory = this.exports.memory.buffer;
    if (this.memoryBuffer && this.memoryBuffer !== memory) this.memoryGrowths += 1;
    this.memoryBuffer = memory;
    const inputPointer = this.exports.shoop_worklet_input_ptr(this.host) >>> 0;
    const outputPointer = this.exports.shoop_worklet_output_ptr(this.host) >>> 0;
    this.input = new Float32Array(memory, inputPointer, MAX_CHANNELS * this.maxQuantum);
    this.output = new Float32Array(memory, outputPointer, MAX_CHANNELS * this.maxQuantum);
    this.commandPointer = this.exports.shoop_worklet_command_ptr(this.host) >>> 0;
  }

  handleCommand(message) {
    if (typeof message !== 'string') {
      this.port.postMessage(JSON.stringify({
        version: PROTOCOL_VERSION,
        sequence: 0,
        event: { kind: 'error', message: 'worklet commands must be JSON strings' },
      }));
      return;
    }
    const encoded = encodeUtf8(message);
    if (encoded.length > MAX_COMMAND_BYTES) {
      this.port.postMessage(JSON.stringify({
        version: PROTOCOL_VERSION,
        sequence: 0,
        event: { kind: 'error', message: 'worklet command exceeds capacity' },
      }));
      return;
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews();
    new Uint8Array(this.exports.memory.buffer, this.commandPointer, encoded.length).set(encoded);
    if (!this.exports.shoop_worklet_command(this.host, encoded.length)) {
      throw new Error('worklet command transport failed');
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews();
    const pointer = this.exports.shoop_worklet_response_ptr(this.host) >>> 0;
    const length = this.exports.shoop_worklet_response_len(this.host) >>> 0;
    let response = decodeUtf8(new Uint8Array(this.exports.memory.buffer, pointer, length));
    const event = JSON.parse(response);
    if (event.event?.kind === 'snapshot') {
      event.event.render_discontinuities = this.renderDiscontinuities;
      event.event.callback_budget_overruns = this.callbackBudgetOverruns;
      event.event.memory_growths = this.memoryGrowths;
      response = JSON.stringify(event);
    }
    this.port.postMessage(response);
  }

  fail(message) {
    this.port.postMessage(JSON.stringify({
      version: PROTOCOL_VERSION,
      sequence: 0,
      event: { kind: 'error', message },
    }));
    return false;
  }

  process(inputs, outputs) {
    if (this.initializationError) return this.fail(this.initializationError);
    const inputChannels = inputs[0] || [];
    const outputChannels = outputs[0] || [];
    const frames = outputChannels[0]?.length || inputChannels[0]?.length || 0;
    if (!frames) return this.fail('AudioWorklet supplied an empty render quantum');
    if (this.expectedFrame !== null && currentFrame !== this.expectedFrame) {
      this.renderDiscontinuities += 1;
    }
    this.expectedFrame = currentFrame + frames;
    if (frames > this.maxQuantum) return this.fail(`render quantum ${frames} exceeds ${this.maxQuantum}`);
    if (this.exports.memory.buffer !== this.memoryBuffer) {
      return this.fail('worklet Wasm memory grew between control and process callbacks');
    }
    const timer = globalThis.performance;
    const startedAt = timer ? timer.now() : 0;
    const nInputs = Math.min(inputChannels.length, MAX_CHANNELS);
    const nOutputs = Math.min(outputChannels.length, MAX_CHANNELS);
    for (let channel = 0; channel < nInputs; channel += 1) {
      const source = inputChannels[channel];
      const offset = channel * this.maxQuantum;
      for (let frame = 0; frame < frames; frame += 1) this.input[offset + frame] = source[frame];
    }
    if (!this.exports.shoop_worklet_process(this.host, nInputs, nOutputs, frames)) {
      return this.fail(`Rust worklet host rejected ${nInputs}x${nOutputs}x${frames} quantum`);
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) {
      return this.fail('worklet Wasm memory grew in the render callback');
    }
    for (let channel = 0; channel < nOutputs; channel += 1) {
      const destination = outputChannels[channel];
      const offset = channel * this.maxQuantum;
      for (let frame = 0; frame < frames; frame += 1) destination[frame] = this.output[offset + frame];
    }
    if (timer && timer.now() - startedAt > frames * 1000 / sampleRate) {
      this.callbackBudgetOverruns += 1;
    }
    return true;
  }
}

registerProcessor('shoop-audio-processor', ShoopAudioProcessor);
