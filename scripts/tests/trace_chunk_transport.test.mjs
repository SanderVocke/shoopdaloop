import assert from 'node:assert/strict';
import test from 'node:test';
import { ShoopTraceChunkTransport } from '../../src/rust/shoopdaloop/raw_wasm_host.js';

class FakeHost {
  constructor(records = 0) {
    this.records = records;
    this.disabled = false;
    this.stopped = false;
  }

  traceAvailableRecords() { return this.records; }
  traceDropped() { return 0; }

  traceDrainInto(destination, offset, maximumBytes) {
    if (this.records === 0 || maximumBytes < 48) return 0;
    destination.fill(this.records & 0xff, offset, offset + 48);
    this.records -= 1;
    return 48;
  }

  traceDisable() { this.disabled = true; }
  traceStop() { this.stopped = true; }
  traceHealth() {
    return {
      emittedRecords: 10,
      droppedRecords: 0,
      completedDrains: 1,
      highWaterRecords: 4,
    };
  }
}

class FakePort {
  constructor() { this.messages = []; }
  postMessage(message, transfer = []) {
    this.messages.push(structuredClone(message, {transfer}));
  }
}

const options = {
  protocolVersion: 2,
  captureId: 9,
  capacityRecords: 1,
  chunkBytes: 48,
  poolSize: 3,
};

function recycle(transport, message) {
  const returned = structuredClone(message.buffer, {transfer: [message.buffer]});
  transport.recycle({
    kind: 'shoop-trace-recycle',
    captureId: message.captureId,
    poolToken: message.poolToken,
    buffer: returned,
  });
}

test('rotates and recycles chunks beyond the producer pool', () => {
  const host = new FakeHost(8);
  const port = new FakePort();
  const transport = new ShoopTraceChunkTransport(host, port, options);
  transport.drain();
  while (host.records > 0) {
    const chunk = port.messages.find(message => message.kind === 'shoop-trace-chunk');
    assert.ok(chunk);
    port.messages.splice(port.messages.indexOf(chunk), 1);
    recycle(transport, chunk);
    transport.drain();
  }
  transport.stop({sourceTicks: 8, referenceMs: 1});
  assert.equal(transport.finished, true);
  const stopped = port.messages.find(message => message.kind === 'shoop-trace-stopped');
  assert.equal(stopped.chunkCount, 8);
  assert.equal(host.stopped, true);
});

test('capture exceeds the former per-realm record retention limit', () => {
  const host = new FakeHost(262_145);
  const port = new FakePort();
  const transport = new ShoopTraceChunkTransport(host, port, {
    ...options,
    chunkBytes: 48 * 1024,
  });
  transport.drain();
  while (host.records > 0) {
    const chunk = port.messages.find(message => message.kind === 'shoop-trace-chunk');
    assert.ok(chunk);
    port.messages.splice(port.messages.indexOf(chunk), 1);
    recycle(transport, chunk);
    transport.drain();
  }
  transport.stop({sourceTicks: 262_145, referenceMs: 1});
  assert.equal(transport.finished, true);
  assert.equal(
    port.messages.find(message => message.kind === 'shoop-trace-stopped').chunkCount,
    257,
  );
});

test('stop during starvation waits for recycle before final drain', () => {
  const host = new FakeHost(4);
  const port = new FakePort();
  const transport = new ShoopTraceChunkTransport(host, port, options);
  transport.drain();
  assert.equal(host.records, 1);
  transport.stop({sourceTicks: 4, referenceMs: 1});
  assert.equal(transport.finished, false);
  assert.equal(host.disabled, true);
  const chunk = port.messages.find(message => message.kind === 'shoop-trace-chunk');
  recycle(transport, chunk);
  assert.equal(host.records, 0);
  assert.equal(transport.finished, true);
  assert.equal(
    port.messages.find(message => message.kind === 'shoop-trace-stopped').chunkCount,
    4,
  );
});

test('empty capture declares zero chunks', () => {
  const host = new FakeHost();
  const port = new FakePort();
  const transport = new ShoopTraceChunkTransport(host, port, options);
  transport.stop({sourceTicks: 0, referenceMs: 1});
  const stopped = port.messages.find(message => message.kind === 'shoop-trace-stopped');
  assert.equal(stopped.chunkCount, 0);
});
