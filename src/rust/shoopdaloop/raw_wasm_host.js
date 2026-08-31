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
    this.trace = null;
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
    if (this.trace) {
      const tracePointer = this.exports.shoop_worklet_trace_ptr(this.host) >>> 0;
      this.trace.bytes = new Uint8Array(
        memory, tracePointer, this.trace.capacityRecords * 48,
      );
    }
  }

  traceStart(realmId, clockId, capacityRecords, engineDetail = false) {
    if (!this.exports.shoop_worklet_trace_start(
      this.host, realmId, clockId, capacityRecords, engineDetail,
    )) {
      throw new Error('Shoop Wasm trace producer could not start');
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) this.refreshViews(false);
    const pointer = this.exports.shoop_worklet_trace_ptr(this.host) >>> 0;
    this.trace = {
      capacityRecords,
      bytes: new Uint8Array(this.exports.memory.buffer, pointer, capacityRecords * 48),
    };
    return this.traceMetadata();
  }

  traceMetadata() {
    const count = this.exports.shoop_worklet_trace_metadata_count() >>> 0;
    const entries = [];
    for (let index = 0; index < count; index += 1) {
      const pointer = this.exports.shoop_worklet_trace_metadata_label_ptr(index) >>> 0;
      const length = this.exports.shoop_worklet_trace_metadata_label_len(index) >>> 0;
      entries.push({
        id: this.exports.shoop_worklet_trace_metadata_id(index) >>> 0,
        namespace: this.exports.shoop_worklet_trace_metadata_namespace(index) >>> 0,
        label: shoopDecodeUtf8(new Uint8Array(this.exports.memory.buffer, pointer, length)),
      });
    }
    return entries;
  }

  traceSetFrame(frame) {
    if (!this.trace) return;
    const low = frame >>> 0;
    const high = Math.floor(frame / 0x1_0000_0000) >>> 0;
    if (!this.exports.shoop_worklet_trace_set_frame(this.host, low, high)) {
      throw new Error('Shoop Wasm trace frame update failed');
    }
  }

  traceDrainInto(destination, offset = 0, maximumBytes = destination.length - offset) {
    if (!this.trace) return 0;
    const available = Math.min(maximumBytes, destination.length - offset);
    const length = this.exports.shoop_worklet_trace_drain(this.host, available) >>> 0;
    if (length > available) {
      throw new RangeError('trace drain destination is too small');
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) {
      this.refreshViews(false);
      const pointer = this.exports.shoop_worklet_trace_ptr(this.host) >>> 0;
      this.trace.bytes = new Uint8Array(
        this.exports.memory.buffer, pointer, this.trace.capacityRecords * 48,
      );
    }
    for (let index = 0; index < length; index += 1) {
      destination[offset + index] = this.trace.bytes[index];
    }
    return length;
  }

  traceDrainRing(destination, writeRecord, maximumRecords, recordBytes = 48) {
    if (!this.trace || maximumRecords === 0) return 0;
    const capacityRecords = Math.floor(destination.length / recordBytes);
    const maximumBytes = Math.min(maximumRecords, capacityRecords) * recordBytes;
    const length = this.exports.shoop_worklet_trace_drain(this.host, maximumBytes) >>> 0;
    if (length > maximumBytes || length % recordBytes) {
      throw new RangeError('invalid trace drain length');
    }
    if (this.exports.memory.buffer !== this.memoryBuffer) {
      this.refreshViews(false);
      const pointer = this.exports.shoop_worklet_trace_ptr(this.host) >>> 0;
      this.trace.bytes = new Uint8Array(
        this.exports.memory.buffer, pointer, this.trace.capacityRecords * recordBytes,
      );
    }
    const records = length / recordBytes;
    const slot = writeRecord % capacityRecords;
    const tailRecords = Math.min(records, capacityRecords - slot);
    const tailBytes = tailRecords * recordBytes;
    destination.set(this.trace.bytes.subarray(0, tailBytes), slot * recordBytes);
    destination.set(this.trace.bytes.subarray(tailBytes, length), 0);
    return records;
  }

  traceDropped() {
    return this.trace ? Number(this.exports.shoop_worklet_trace_dropped(this.host)) : 0;
  }

  traceHealth() {
    return this.trace ? {
      emittedRecords: Number(this.exports.shoop_worklet_trace_emitted(this.host)),
      droppedRecords: Number(this.exports.shoop_worklet_trace_dropped(this.host)),
      completedDrains: Number(this.exports.shoop_worklet_trace_completed_drains(this.host)),
      highWaterRecords: Number(this.exports.shoop_worklet_trace_high_water(this.host)),
    } : {emittedRecords: 0, droppedRecords: 0, completedDrains: 0, highWaterRecords: 0};
  }

  traceAvailableRecords() {
    return this.trace ? Number(this.exports.shoop_worklet_trace_available(this.host)) : 0;
  }

  traceDisable() {
    if (this.trace) this.exports.shoop_worklet_trace_disable(this.host);
  }

  traceStop() {
    if (!this.trace) return;
    this.exports.shoop_worklet_trace_stop(this.host);
    this.trace = null;
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
    this.traceStop();
    this.exports.shoop_worklet_destroy(this.host);
    this.host = 0;
    this.input = null;
    this.output = null;
    this.memoryBuffer = null;
  }
}

export class ShoopTraceChunkTransport {
  constructor(host, port, options) {
    if (options.protocolVersion !== 2) throw new Error('trace chunk protocol version mismatch');
    if (!Number.isInteger(options.captureId) || options.captureId <= 0) {
      throw new Error('trace capture ID must be positive');
    }
    if (!Number.isInteger(options.poolSize) || options.poolSize < 2) {
      throw new Error('trace chunk pool must contain at least two buffers');
    }
    const minimumBytes = options.capacityRecords * 48;
    if (!Number.isInteger(options.chunkBytes)
        || options.chunkBytes < minimumBytes
        || options.chunkBytes % 48 !== 0) {
      throw new Error('trace chunk capacity cannot hold the largest raw record group');
    }
    this.host = host;
    this.port = port;
    this.captureId = options.captureId;
    this.chunkBytes = options.chunkBytes;
    this.available = [];
    for (let token = 0; token < options.poolSize; token += 1) {
      const buffer = new ArrayBuffer(this.chunkBytes);
      this.available.push({token, buffer, view: new Uint8Array(buffer), used: 0});
    }
    this.active = this.available.pop();
    this.sequence = 0;
    this.inFlight = new Array(options.poolSize).fill(false);
    this.inFlightCount = 0;
    this.maxInFlight = 0;
    this.returnedBuffers = 0;
    this.poolStarvationRecords = 0;
    this.starvationDropStart = null;
    this.rejectedChunks = 0;
    this.stopping = null;
    this.finished = false;
  }

  drain() {
    while (this.host.traceAvailableRecords() > 0) {
      if (!this.active) this.active = this.available.pop() || null;
      if (!this.active) {
        if (this.starvationDropStart === null) {
          this.starvationDropStart = this.host.traceDropped();
        }
        return false;
      }
      const remaining = this.chunkBytes - this.active.used;
      const bytes = this.host.traceDrainInto(this.active.view, this.active.used, remaining);
      if (bytes > 0) this.active.used += bytes;
      if (this.active.used === this.chunkBytes) {
        this.transferActive();
      } else if (bytes === 0 && this.host.traceAvailableRecords() > 0) {
        // The next complete group fits an empty chunk but not this remaining tail.
        this.transferActive();
      }
    }
    return true;
  }

  transferActive() {
    if (!this.active || this.active.used === 0) return;
    const chunk = this.active;
    this.active = this.available.pop() || null;
    this.inFlight[chunk.token] = true;
    this.inFlightCount += 1;
    this.maxInFlight = Math.max(this.maxInFlight, this.inFlightCount);
    this.port.postMessage({
      kind: 'shoop-trace-chunk',
      captureId: this.captureId,
      sequence: this.sequence,
      poolToken: chunk.token,
      usedBytes: chunk.used,
      buffer: chunk.buffer,
    }, [chunk.buffer]);
    this.sequence += 1;
  }

  recycle(message) {
    if (message.captureId !== this.captureId || this.finished) return false;
    if (!Number.isInteger(message.poolToken) || !this.inFlight[message.poolToken]) {
      this.rejectedChunks += 1;
      throw new Error('trace recycle has an unknown or duplicate pool token');
    }
    this.inFlight[message.poolToken] = false;
    this.inFlightCount -= 1;
    this.returnedBuffers += 1;
    if (this.starvationDropStart !== null) {
      this.poolStarvationRecords += this.host.traceDropped() - this.starvationDropStart;
      this.starvationDropStart = null;
    }
    if (!(message.buffer instanceof ArrayBuffer) || message.buffer.byteLength !== this.chunkBytes) {
      throw new Error('trace recycle buffer capacity mismatch');
    }
    this.available.push({
      token: message.poolToken,
      buffer: message.buffer,
      view: new Uint8Array(message.buffer),
      used: 0,
    });
    if (!this.active) this.active = this.available.pop();
    if (this.stopping) this.tryFinish();
    return true;
  }

  stop(status) {
    if (this.stopping || this.finished) return;
    this.host.traceDisable();
    this.stopping = status;
    this.tryFinish();
  }

  tryFinish() {
    if (!this.stopping || this.finished || !this.drain()) return false;
    this.transferActive();
    const health = this.host.traceHealth();
    this.host.traceStop();
    this.finished = true;
    this.port.postMessage({
      kind: 'shoop-trace-stopped',
      captureId: this.captureId,
      chunkCount: this.sequence,
      emittedRecords: health.emittedRecords,
      droppedRecords: health.droppedRecords,
      rawDroppedRecords: health.droppedRecords - this.poolStarvationRecords,
      poolStarvationRecords: this.poolStarvationRecords,
      highWaterRecords: health.highWaterRecords,
      maxInFlight: this.maxInFlight,
      returnedBuffers: this.returnedBuffers,
      rejectedChunks: this.rejectedChunks,
      ...this.stopping,
    });
    return true;
  }

  abort(status = {}) {
    if (this.finished) return;
    this.host.traceDisable();
    this.host.traceStop();
    this.finished = true;
    try {
      this.port.postMessage({
        kind: 'shoop-trace-aborted',
        captureId: this.captureId,
        ...status,
      });
    } catch (_) { /* producer realm is terminating */ }
  }
}
