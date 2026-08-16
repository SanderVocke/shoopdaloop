let ownedWorkers = 0;
let ownedChannels = 0;
let ownedBlobUrls = 0;

function withTimeout(register, remove, timeout, label) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      remove();
      reject(new Error(`${label} timed out`));
    }, timeout);
    register(value => {
      clearTimeout(timer);
      remove();
      resolve(value?.data ?? value);
    });
  });
}

function nextMessage(target, timeout = 5000) {
  if (typeof target.addEventListener === 'function') {
    let handler;
    return withTimeout(
      done => {
        handler = event => done(event);
        target.addEventListener('message', handler);
        target.start?.();
      },
      () => target.removeEventListener('message', handler),
      timeout,
      'Worker fixture message',
    );
  }
  let handler;
  return withTimeout(
    done => {
      handler = value => done(value);
      target.on('message', handler);
    },
    () => target.off('message', handler),
    timeout,
    'Node Worker fixture message',
  );
}

async function runtimeApi(runtime, assetLocation) {
  if (runtime === 'node') {
    const [{ Worker, MessageChannel }, fs, url, path] = await Promise.all([
      import('node:worker_threads'),
      import('node:fs/promises'),
      import('node:url'),
      import('node:path'),
    ]);
    const asset = name => path.join(assetLocation, name);
    return {
      Worker,
      MessageChannel,
      workerUrl: url.pathToFileURL(asset('node_worker_bootstrap.mjs')),
      workerOptions: {
        type: 'module',
        workerData: { workerModuleUrl: url.pathToFileURL(asset('audio_worker.js')).href },
      },
      module: WebAssembly.compile(await fs.readFile(asset('shoop_audio_worklet.wasm'))),
      cleanup: () => {},
      waitForBootstrap: true,
      waitForExit(worker) {
        return new Promise((resolve, reject) => {
          const timer = setTimeout(() => reject(new Error('Node Worker exit timed out')), 5000);
          worker.once('exit', code => {
            clearTimeout(timer);
            if (code === 0) resolve();
            else reject(new Error(`Node Worker exited with ${code}`));
          });
        });
      },
    };
  }

  const [hostResponse, workerResponse, wasmResponse] = await Promise.all([
    fetch(`${assetLocation}/raw_wasm_host.js`),
    fetch(`${assetLocation}/audio_worker.js`),
    fetch(`${assetLocation}/shoop_audio_worklet.wasm`),
  ]);
  for (const response of [hostResponse, workerResponse, wasmResponse]) {
    if (!response.ok) throw new Error(`test asset fetch failed: ${response.url} ${response.status}`);
  }
  const hostSource = await hostResponse.text();
  const hostUrl = URL.createObjectURL(new Blob([hostSource], { type: 'text/javascript' }));
  ownedBlobUrls += 1;
  const workerSource = (await workerResponse.text()).replace(
    "'./raw_wasm_host.js'",
    JSON.stringify(hostUrl),
  );
  const workerUrl = URL.createObjectURL(new Blob([workerSource], { type: 'text/javascript' }));
  ownedBlobUrls += 1;
  return {
    Worker,
    MessageChannel,
    workerUrl,
    workerOptions: { type: 'module' },
    module: WebAssembly.compile(await wasmResponse.arrayBuffer()),
    cleanup() {
      URL.revokeObjectURL(workerUrl);
      URL.revokeObjectURL(hostUrl);
      ownedBlobUrls -= 2;
    },
    waitForBootstrap: false,
    waitForExit: null,
  };
}

async function boot(api, protocolVersion, commandMaxBytes, mode = 'explicit') {
  const worker = new api.Worker(api.workerUrl, api.workerOptions);
  if (api.waitForBootstrap) {
    const message = await nextMessage(worker);
    if (message.kind !== 'node-bootstrap-ready') throw new Error('Node bootstrap did not become ready');
  }
  const application = new api.MessageChannel();
  const fixture = new api.MessageChannel();
  worker.postMessage({
    kind: 'initialize',
    wasmModule: await api.module,
    applicationPort: application.port2,
    fixturePort: fixture.port2,
    sampleRate: 48000,
    quantum: 128,
    maxQuantum: 2048,
    protocolVersion,
    commandMaxBytes,
    processingMode: mode,
  }, [application.port2, fixture.port2]);
  const ready = await nextMessage(fixture.port1);
  if (ready.kind !== 'ready' || ready.diagnostics.mode !== mode) {
    throw new Error(`production Worker did not become ready: ${JSON.stringify(ready)}`);
  }
  ownedWorkers += 1;
  ownedChannels += 2;
  return {
    worker,
    application: application.port1,
    fixture: fixture.port1,
    released: false,
  };
}

async function fixtureCommand(instance, command) {
  const response = nextMessage(instance.fixture);
  instance.fixture.postMessage(command);
  return response;
}

async function poll(instance, protocolVersion, sequence) {
  const response = nextMessage(instance.application);
  instance.application.postMessage(JSON.stringify({
    version: protocolVersion,
    sequence,
    command: { kind: 'poll' },
  }));
  const envelope = JSON.parse(await response);
  if (envelope.sequence !== sequence || envelope.event?.kind !== 'snapshot') {
    throw new Error(`production protocol mismatch: ${JSON.stringify(envelope)}`);
  }
  return envelope.event;
}

function releaseAccounting(instance) {
  if (instance.released) return;
  instance.released = true;
  ownedWorkers -= 1;
  ownedChannels -= 2;
}

async function stop(api, instance) {
  const exited = api.waitForExit?.(instance.worker);
  try {
    const stopped = await fixtureCommand(instance, { kind: 'shutdown' });
    if (stopped.kind !== 'stopped') throw new Error('fixture shutdown was not acknowledged');
    instance.application.close();
    instance.fixture.close();
    if (exited) await exited;
    else instance.worker.terminate();
  } finally {
    releaseAccounting(instance);
  }
}

function assertNoLeaks() {
  if (ownedWorkers || ownedChannels || ownedBlobUrls) {
    throw new Error(
      `fixture leak: Workers=${ownedWorkers}, channels=${ownedChannels}, Blob URLs=${ownedBlobUrls}`,
    );
  }
}

async function applicationCommand(instance, protocolVersion, sequence, command) {
  const response = nextMessage(instance.application);
  instance.application.postMessage(JSON.stringify({ version: protocolVersion, sequence, command }));
  const envelope = JSON.parse(await response);
  if (envelope.sequence !== sequence) {
    throw new Error(`response sequence mismatch: ${JSON.stringify(envelope)}`);
  }
  return envelope.event;
}

async function waitForProgress(instance, minimum, timeout = 5000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const state = await fixtureCommand(instance, { kind: 'state' });
    if (state.kind === 'state' && state.diagnostics.processedQuanta >= minimum) return state;
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  throw new Error(`Worker did not reach ${minimum} processed quanta`);
}

async function productionStop(api, instance, protocolVersion, sequence) {
  const exited = api.waitForExit?.(instance.worker);
  try {
    const applicationStopped = nextMessage(instance.application);
    const fixtureStopped = nextMessage(instance.fixture);
    instance.application.postMessage(JSON.stringify({
      version: protocolVersion,
      sequence,
      command: { kind: 'shutdown' },
    }));
    const [applicationEnvelope, fixtureEvent] = await Promise.all([
      applicationStopped.then(JSON.parse),
      fixtureStopped,
    ]);
    if (applicationEnvelope.event?.kind !== 'stopped' || fixtureEvent.kind !== 'stopped') {
      throw new Error('production shutdown did not notify both control surfaces');
    }
    instance.application.close();
    instance.fixture.close();
    if (exited) await exited;
    else instance.worker.terminate();
  } finally {
    releaseAccounting(instance);
  }
}

function subscribeMessages(target, callback) {
  if (typeof target.addEventListener === 'function') {
    const handler = event => callback(event.data);
    target.addEventListener('message', handler);
    target.start?.();
    return () => target.removeEventListener('message', handler);
  }
  const handler = value => callback(value);
  target.on('message', handler);
  return () => target.off('message', handler);
}

export async function spawnRemoteApplicationFixture(
  runtime,
  assetLocation,
  protocolVersion,
  commandMaxBytes,
  onMessage,
) {
  const api = await runtimeApi(runtime, assetLocation);
  try {
    const instance = await boot(api, protocolVersion, commandMaxBytes, 'explicit');
    return {
      api,
      instance,
      unsubscribe: subscribeMessages(instance.application, onMessage),
      closed: false,
    };
  } catch (error) {
    api.cleanup();
    throw error;
  }
}

export function remoteApplicationPostMessage(fixture, message) {
  if (fixture.closed) throw new Error('remote application fixture is closed');
  fixture.instance.application.postMessage(message);
}

export async function remoteApplicationProcessQuantum(fixture, inputs, outputChannels) {
  if (fixture.closed) throw new Error('remote application fixture is closed');
  return fixtureCommand(fixture.instance, {
    kind: 'process',
    frames: 128,
    inputs,
    outputChannels,
  });
}

export async function remoteApplicationTurn() {
  await new Promise(resolve => setTimeout(resolve, 0));
}

export async function shutdownRemoteApplicationFixture(fixture) {
  if (fixture.closed) return;
  fixture.closed = true;
  fixture.unsubscribe();
  try {
    await stop(fixture.api, fixture.instance);
  } finally {
    fixture.api.cleanup();
  }
}

export class MultiWorkerFixture {
  static async spawn(runtime, assetLocation, protocolVersion, commandMaxBytes, modes) {
    const api = await runtimeApi(runtime, assetLocation);
    const fixture = new MultiWorkerFixture(api, protocolVersion, commandMaxBytes);
    try {
      for (const mode of modes) {
        fixture.instances.push(await boot(api, protocolVersion, commandMaxBytes, mode));
      }
      return fixture;
    } catch (error) {
      await fixture.shutdown().catch(() => {});
      throw error;
    }
  }

  constructor(api, protocolVersion, commandMaxBytes) {
    this.api = api;
    this.protocolVersion = protocolVersion;
    this.commandMaxBytes = commandMaxBytes;
    this.instances = [];
    this.closed = false;
  }

  worker(index) {
    const instance = this.instances[index];
    if (!instance) throw new Error(`unknown fixture Worker ${index}`);
    return instance;
  }

  processQuantum(index, inputs = [], outputChannels = 0) {
    return fixtureCommand(this.worker(index), {
      kind: 'process', frames: 128, inputs, outputChannels,
    });
  }

  poll(index, sequence) {
    return poll(this.worker(index), this.protocolVersion, sequence);
  }

  state(index) {
    return fixtureCommand(this.worker(index), { kind: 'state' });
  }

  async shutdown() {
    if (this.closed) return;
    this.closed = true;
    const instances = this.instances.splice(0);
    try {
      await Promise.all(instances.map(instance => stop(this.api, instance)));
    } finally {
      this.api.cleanup();
      assertNoLeaks();
    }
  }
}

export async function runProductionWorkerProbe(
  runtime,
  assetLocation,
  protocolVersion,
  commandMaxBytes,
) {
  const fixture = await MultiWorkerFixture.spawn(
    runtime,
    assetLocation,
    protocolVersion,
    commandMaxBytes,
    ['explicit', 'explicit'],
  );
  try {
    let leakDetected = false;
    try {
      assertNoLeaks();
    } catch (error) {
      leakDetected = /fixture leak/.test(String(error));
    }
    if (!leakDetected) throw new Error('active fixture ownership was not detected');
    const [firstInitial, secondInitial] = await Promise.all([
      fixture.poll(0, 1),
      fixture.poll(1, 1),
    ]);
    if (firstInitial.callback_count !== 0 || secondInitial.callback_count !== 0) {
      throw new Error('fresh production Workers shared progression');
    }
    const processed = await fixture.processQuantum(
      0,
      [[0.25, ...Array(127).fill(0)]],
      2,
    );
    if (processed.kind !== 'processed'
        || processed.outputs.length !== 2
        || processed.diagnostics.processedQuanta !== 1) {
      throw new Error(`explicit production processing failed: ${JSON.stringify(processed)}`);
    }
    const [advanced, isolated] = await Promise.all([
      fixture.poll(0, 2),
      fixture.poll(1, 2),
    ]);
    if (advanced.callback_count !== 1 || isolated.callback_count !== 0) {
      throw new Error('production Worker instances leaked engine state');
    }
    return 'production Worker probe: ok';
  } finally {
    await fixture.shutdown();
  }
}

export async function runProcessingModeContracts(
  runtime,
  assetLocation,
  protocolVersion,
  commandMaxBytes,
) {
  const api = await runtimeApi(runtime, assetLocation);
  const instances = [];
  try {
    for (const mode of ['explicit', 'cooperative', 'realtime']) {
      const instance = await boot(api, protocolVersion, commandMaxBytes, mode);
      instances.push(instance);
      if (mode === 'explicit') {
        const first = await fixtureCommand(instance, {
          kind: 'process',
          frames: 128,
          inputs: [[0.5, ...Array(127).fill(0)]],
          outputChannels: 2,
        });
        const second = await fixtureCommand(instance, {
          kind: 'process',
          frames: 128,
          outputChannels: 2,
        });
        if (first.diagnostics?.processedQuanta !== 1 || second.diagnostics?.processedQuanta !== 2) {
          throw new Error('explicit processing was not deterministic');
        }
      } else {
        const progressed = await waitForProgress(instance, 1);
        const paused = await fixtureCommand(instance, { kind: 'pause' });
        if (paused.kind !== 'state' || paused.diagnostics.running) {
          throw new Error(`${mode} Worker did not pause`);
        }
        await new Promise(resolve => setTimeout(resolve, 20));
        const retained = await fixtureCommand(instance, { kind: 'state' });
        if (retained.diagnostics.processedQuanta !== paused.diagnostics.processedQuanta) {
          throw new Error(`${mode} Worker progressed while paused`);
        }
        await fixtureCommand(instance, { kind: 'resume' });
        await waitForProgress(instance, progressed.diagnostics.processedQuanta + 1);
      }
      await poll(instance, protocolVersion, 1);
      await stop(api, instances.pop());
      const restarted = await boot(api, protocolVersion, commandMaxBytes, mode);
      instances.push(restarted);
      const fresh = await poll(restarted, protocolVersion, 1);
      if (mode === 'explicit' && fresh.callback_count !== 0) {
        throw new Error('explicit Worker restart retained callback state');
      }
      await stop(api, instances.pop());
    }
    return 'processing mode contracts: ok';
  } finally {
    await Promise.all(instances.map(instance => stop(api, instance).catch(() => {})));
    api.cleanup();
    assertNoLeaks();
  }
}

export async function runProtocolAndShutdownContracts(
  runtime,
  assetLocation,
  protocolVersion,
  commandMaxBytes,
) {
  const api = await runtimeApi(runtime, assetLocation);
  const instances = [];
  try {
    const instance = await boot(api, protocolVersion, commandMaxBytes);
    instances.push(instance);
    const first = await applicationCommand(instance, protocolVersion, 1, { kind: 'poll' });
    const skipped = await applicationCommand(instance, protocolVersion, 3, { kind: 'poll' });
    const recovered = await applicationCommand(instance, protocolVersion, 2, { kind: 'poll' });
    if (first.kind !== 'snapshot' || skipped.kind !== 'error' || recovered.kind !== 'snapshot') {
      throw new Error(
        `out-of-order sequence contract failed: ${JSON.stringify({ first, skipped, recovered })}`,
      );
    }
    const created = await applicationCommand(instance, protocolVersion, 3, {
      kind: 'create_track',
      expected_track_id: 1,
      expected_loop_ids: [1],
      port_name_base: 'wasm-fixture',
      topology: { kind: 'direct', audio_channels: 0, midi: true },
    });
    if (created.kind !== 'ack') throw new Error(`track creation failed: ${JSON.stringify(created)}`);
    const midi = await applicationCommand(instance, protocolVersion, 4, {
      kind: 'inject_track_midi_input',
      track_id: 1,
      events: [{ frame: 0, data: [144, 60, 100] }],
    });
    if (midi.kind !== 'ack') throw new Error(`MIDI injection failed: ${JSON.stringify(midi)}`);
    const processed = await fixtureCommand(instance, {
      kind: 'process',
      frames: 128,
      outputChannels: 0,
    });
    if (processed.kind !== 'processed') throw new Error('MIDI quantum did not process');
    const snapshot = await poll(instance, protocolVersion, 5);
    if (snapshot.callback_count !== 1 || snapshot.tracks?.length !== 1) {
      throw new Error(`protocol state was not observable: ${JSON.stringify(snapshot)}`);
    }
    await productionStop(api, instances.pop(), protocolVersion, 6);
    return 'protocol and shutdown contracts: ok';
  } finally {
    await Promise.all(instances.map(instance => stop(api, instance).catch(() => {})));
    api.cleanup();
    assertNoLeaks();
  }
}

export async function runFailureIsolationContracts(
  runtime,
  assetLocation,
  protocolVersion,
  commandMaxBytes,
) {
  const api = await runtimeApi(runtime, assetLocation);
  const instances = [];
  try {
    const doomed = await boot(api, protocolVersion, commandMaxBytes);
    instances.push(doomed);
    const survivor = await boot(api, protocolVersion, commandMaxBytes);
    instances.push(survivor);
    const exited = api.waitForExit?.(doomed.worker);
    const applicationFailure = nextMessage(doomed.application);
    const fixtureFailure = nextMessage(doomed.fixture);
    doomed.application.postMessage('x'.repeat(commandMaxBytes + 1));
    const [applicationEnvelope, fixtureEvent] = await Promise.all([
      applicationFailure.then(JSON.parse),
      fixtureFailure,
    ]);
    if (applicationEnvelope.sequence !== 0
        || applicationEnvelope.event?.kind !== 'error'
        || fixtureEvent.kind !== 'failure') {
      throw new Error('terminal Worker failure was not typed on both surfaces');
    }
    doomed.application.close();
    doomed.fixture.close();
    if (exited) await exited;
    else doomed.worker.terminate();
    releaseAccounting(doomed);
    instances.shift();

    const processed = await fixtureCommand(survivor, {
      kind: 'process', frames: 128, outputChannels: 0,
    });
    const snapshot = await poll(survivor, protocolVersion, 1);
    if (processed.kind !== 'processed' || snapshot.callback_count !== 1) {
      throw new Error('peer Worker stopped after isolated failure');
    }
    await stop(api, instances.pop());
    return 'failure isolation contracts: ok';
  } finally {
    await Promise.all(instances.map(instance => stop(api, instance).catch(() => {})));
    api.cleanup();
    assertNoLeaks();
  }
}
