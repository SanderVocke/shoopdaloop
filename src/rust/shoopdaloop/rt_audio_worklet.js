// AudioWorkletProcessor for the new engine's import-free Wasm module. It copies
// samples and posted bytes in and out; the module owns all behaviour.
const RECORDS_IN = 0;
const RECORDS_OUT = 1;
const MIDI_IN = 2;
const MIDI_OUT = 3;
const PREPARED = 0;
const RENDERED = 0;
const FAULTED = 3;
const COUNTERS = [
  'discontinuities',
  'overruns',
  'midiInDropped',
  'midiInLate',
  'midiOutDropped',
  'framingErrors',
];

function bytesOf(data) {
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  throw new TypeError('expected an ArrayBuffer or a typed array');
}

class ShoopRtAudioProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.inbound = { [RECORDS_IN]: [], [MIDI_IN]: [] };
    this.load = -1;
    this.port.onmessage = event => this.receive(event.data);
    try {
      const { wasmModule, config } = options.processorOptions;
      this.wasm = new WebAssembly.Instance(wasmModule, {}).exports;
      const record = bytesOf(config);
      const pointer = this.wasm.shoop_rt_worklet_config_buffer(record.length) >>> 0;
      if (!pointer) throw new RangeError('the configuration record is empty or too large');
      new Uint8Array(this.wasm.memory.buffer, pointer, record.length).set(record);
      const code = this.wasm.shoop_rt_worklet_prepare();
      if (code !== PREPARED) {
        this.fail('prepare', code, 'the module refused its configuration');
        return;
      }
      this.maxFrames = this.wasm.shoop_rt_worklet_max_frames();
      this.audioInputs = this.wasm.shoop_rt_worklet_audio_inputs();
      this.audioOutputs = this.wasm.shoop_rt_worklet_audio_outputs();
      this.views();
    } catch (error) {
      this.fail('instantiate', 0, String(error?.stack || error));
    }
  }

  views() {
    const wasm = this.wasm;
    this.memory = wasm.memory.buffer;
    this.bytes = new Uint8Array(this.memory);
    this.input = new Float32Array(
      this.memory,
      wasm.shoop_rt_worklet_audio_input_ptr() >>> 0,
      this.audioInputs * this.maxFrames,
    );
    this.output = new Float32Array(
      this.memory,
      wasm.shoop_rt_worklet_audio_output_ptr() >>> 0,
      this.audioOutputs * this.maxFrames,
    );
    this.pointers = [0, 1, 2, 3].map(id => wasm.shoop_rt_worklet_buffer_ptr(id) >>> 0);
    this.capacities = [0, 1, 2, 3].map(id => wasm.shoop_rt_worklet_buffer_capacity(id) >>> 0);
  }

  receive(data) {
    switch (data?.kind) {
      case 'records':
        this.inbound[RECORDS_IN].push(bytesOf(data.bytes));
        break;
      case 'midi':
        this.inbound[MIDI_IN].push(bytesOf(data.bytes));
        break;
      case 'status':
        this.port.postMessage({ kind: 'status', counters: this.counters() });
        break;
      case 'stop':
        this.stopped = true;
        break;
      default:
        break;
    }
  }

  counters() {
    const counters = {};
    COUNTERS.forEach((name, id) => {
      counters[name] = this.failed ? 0 : this.wasm.shoop_rt_worklet_counter(id) >>> 0;
    });
    return counters;
  }

  // Copy queued messages into an inbound buffer as far as it has room.
  feed(id) {
    const queue = this.inbound[id];
    while (queue.length) {
      const length = this.wasm.shoop_rt_worklet_buffer_len(id) >>> 0;
      const count = Math.min(this.capacities[id] - length, queue[0].length);
      if (count <= 0) return;
      this.bytes.set(queue[0].subarray(0, count), this.pointers[id] + length);
      this.wasm.shoop_rt_worklet_commit(id, count);
      if (count === queue[0].length) queue.shift();
      else queue[0] = queue[0].subarray(count);
    }
  }

  post(id, kind) {
    const length = this.wasm.shoop_rt_worklet_buffer_len(id) >>> 0;
    if (!length) return;
    const bytes = this.bytes.slice(this.pointers[id], this.pointers[id] + length);
    this.port.postMessage({ kind, bytes: bytes.buffer }, [bytes.buffer]);
  }

  fail(stage, code, message) {
    if (!this.failed) {
      this.failed = true;
      this.port.postMessage({ kind: 'failed', stage, code, message });
      this.wasm?.shoop_rt_worklet_release();
    }
    return false;
  }

  process(inputs, outputs) {
    if (this.failed) return false;
    if (this.stopped) {
      this.wasm.shoop_rt_worklet_release();
      this.failed = true;
      return false;
    }
    const timer = globalThis.performance;
    const started = timer ? timer.now() : 0;
    const input = inputs[0] || [];
    const output = outputs[0] || [];
    const frames = output[0]?.length || input[0]?.length || 128;
    try {
      if (this.wasm.memory.buffer !== this.memory) this.views();
      this.feed(RECORDS_IN);
      this.feed(MIDI_IN);
      const inputChannels = Math.min(input.length, this.audioInputs);
      for (let channel = 0; channel < inputChannels; channel += 1) {
        this.input.set(input[channel], channel * this.maxFrames);
      }
      const code = this.wasm.shoop_rt_worklet_process(
        frames,
        inputChannels,
        currentFrame,
        this.load,
      );
      if (code !== RENDERED && code !== FAULTED) {
        return this.fail('process', code, `the module refused a quantum of ${frames} frames`);
      }
      if (this.wasm.memory.buffer !== this.memory) this.views();
      const outputChannels = Math.min(output.length, this.audioOutputs);
      for (let channel = 0; channel < outputChannels; channel += 1) {
        const start = channel * this.maxFrames;
        output[channel].set(this.output.subarray(start, start + frames));
      }
      this.post(RECORDS_OUT, 'records');
      this.post(MIDI_OUT, 'midi');
    } catch (error) {
      return this.fail('process', 0, String(error?.stack || error));
    }
    if (timer) this.load = ((timer.now() - started) * sampleRate) / (1000 * frames);
    return true;
  }
}

registerProcessor('shoop-rt-audio-processor', ShoopRtAudioProcessor);
