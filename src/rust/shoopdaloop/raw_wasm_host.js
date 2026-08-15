// Adapter-neutral bridge for the import-free Shoop engine Wasm ABI.
// AudioWorkletGlobalScope does not consistently expose TextEncoder/TextDecoder,
// so keep UTF-8 conversion self-contained.
function shoopEncodeUtf8(value) {
  const bytes = [];
  for (const character of value) {
    const code = character.codePointAt(0);
    if (code < 0x80) bytes.push(code);
    else if (code < 0x800) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    else if (code < 0x10000) {
      bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    } else {
      bytes.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    }
  }
  return new Uint8Array(bytes);
}

function shoopDecodeUtf8(bytes) {
  let output = '';
  for (let index = 0; index < bytes.length;) {
    const first = bytes[index++];
    let code;
    if (first < 0x80) code = first;
    else if (first < 0xe0) code = ((first & 0x1f) << 6) | (bytes[index++] & 0x3f);
    else if (first < 0xf0) {
      code = ((first & 0x0f) << 12)
        | ((bytes[index++] & 0x3f) << 6)
        | (bytes[index++] & 0x3f);
    } else {
      code = ((first & 7) << 18)
        | ((bytes[index++] & 0x3f) << 12)
        | ((bytes[index++] & 0x3f) << 6)
        | (bytes[index++] & 0x3f);
    }
    output += String.fromCodePoint(code);
  }
  return output;
}

export class ShoopRawWasmHost {
  constructor(wasmModule, sampleRate, maxQuantum, commandMaxBytes) {
    this.maxQuantum = maxQuantum;
    this.commandMaxBytes = commandMaxBytes;
    this.instance = new WebAssembly.Instance(wasmModule, {});
    this.exports = this.instance.exports;
    this.host = this.exports.shoop_worklet_create(sampleRate, maxQuantum);
    if (!this.host) throw new Error('could not create the Shoop Wasm host');
    this.memoryBuffer = null;
    this.memoryGrowths = 0;
    this.renderMemoryGrowths = 0;
    this.refreshViews(false);
  }

  refreshViews(rendering) {
    if (!this.host) throw new Error('Shoop Wasm host is destroyed');
    const memory = this.exports.memory.buffer;
    if (this.memoryBuffer && this.memoryBuffer !== memory) {
      this.memoryGrowths += 1;
      if (rendering) this.renderMemoryGrowths += 1;
    }
    this.memoryBuffer = memory;
    const inputPointer = this.exports.shoop_worklet_input_ptr(this.host) >>> 0;
    const outputPointer = this.exports.shoop_worklet_output_ptr(this.host) >>> 0;
    this.commandPointer = this.exports.shoop_worklet_command_ptr(this.host) >>> 0;
    this.input = new Float32Array(memory, inputPointer, 2 * this.maxQuantum);
    this.output = new Float32Array(memory, outputPointer, 2 * this.maxQuantum);
    if (this.commandPointer + this.commandMaxBytes > memory.byteLength) {
      throw new Error('Shoop Wasm command buffer is outside linear memory');
    }
  }

  command(message) {
    if (typeof message !== 'string') throw new TypeError('Shoop commands must be JSON strings');
    const encoded = shoopEncodeUtf8(message);
    if (encoded.length > this.commandMaxBytes) throw new RangeError('Shoop command exceeds capacity');
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews(false);
    new Uint8Array(this.exports.memory.buffer, this.commandPointer, encoded.length).set(encoded);
    if (!this.exports.shoop_worklet_command(this.host, encoded.length)) {
      throw new Error('Shoop Wasm command transport failed');
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews(false);
    const pointer = this.exports.shoop_worklet_response_ptr(this.host) >>> 0;
    const length = this.exports.shoop_worklet_response_len(this.host) >>> 0;
    if (pointer + length > this.exports.memory.buffer.byteLength) {
      throw new Error('Shoop Wasm response is outside linear memory');
    }
    const response = shoopDecodeUtf8(
      new Uint8Array(this.exports.memory.buffer, pointer, length),
    );
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews(false);
    return response;
  }

  process(inputChannels, outputChannels, frames) {
    if (!Number.isInteger(frames) || frames <= 0 || frames > this.maxQuantum) {
      throw new RangeError(`invalid Shoop render quantum ${frames}`);
    }
    if (inputChannels.length > 2 || outputChannels.length > 2) {
      throw new RangeError('Shoop host supports at most two device channels');
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews(false);
    for (let channel = 0; channel < inputChannels.length; channel += 1) {
      const source = inputChannels[channel];
      if (source.length < frames) throw new RangeError('Shoop input channel is too short');
      this.input.set(source.subarray(0, frames), channel * this.maxQuantum);
    }
    if (!this.exports.shoop_worklet_process(
      this.host,
      inputChannels.length,
      outputChannels.length,
      frames,
    )) {
      throw new Error('Shoop Wasm host rejected the render quantum');
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews(true);
    for (let channel = 0; channel < outputChannels.length; channel += 1) {
      const destination = outputChannels[channel];
      if (destination.length < frames) throw new RangeError('Shoop output channel is too short');
      destination.set(
        this.output.subarray(channel * this.maxQuantum, channel * this.maxQuantum + frames),
      );
    }
  }

  diagnostics() {
    return {
      memoryGrowths: this.memoryGrowths,
      renderMemoryGrowths: this.renderMemoryGrowths,
    };
  }

  destroy() {
    if (!this.host) return;
    this.exports.shoop_worklet_destroy(this.host);
    this.host = 0;
    this.input = null;
    this.output = null;
    this.memoryBuffer = null;
  }
}
