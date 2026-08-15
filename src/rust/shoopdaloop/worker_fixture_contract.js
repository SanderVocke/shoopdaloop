function nextMessage(port, timeout = 5000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Worker fixture message timed out')), timeout);
    port.addEventListener('message', function handler(event) {
      clearTimeout(timer);
      port.removeEventListener('message', handler);
      resolve(event.data);
    });
    port.start();
  });
}

async function boot(module, mode) {
  const worker = new Worker('./audio_worker.js');
  const application = new MessageChannel();
  const fixture = new MessageChannel();
  worker.postMessage({
    kind: 'initialize',
    wasmModule: module,
    applicationPort: application.port2,
    fixturePort: fixture.port2,
    sampleRate: 48000,
    quantum: 128,
    maxQuantum: 2048,
    protocolVersion: 12,
    commandMaxBytes: 262144,
    processingMode: mode,
  }, [application.port2, fixture.port2]);
  const ready = await nextMessage(fixture.port1);
  if (ready.kind !== 'ready' || ready.diagnostics.mode !== mode) {
    throw new Error(`Worker ${mode} did not become ready`);
  }
  return { worker, application: application.port1, fixture: fixture.port1 };
}

async function poll(instance, sequence) {
  const response = nextMessage(instance.application);
  instance.application.postMessage(JSON.stringify({
    version: 12,
    sequence,
    command: { kind: 'poll' },
  }));
  const envelope = JSON.parse(await response);
  if (envelope.sequence !== sequence || envelope.event?.kind !== 'snapshot') {
    throw new Error(`Worker application protocol mismatch: ${JSON.stringify(envelope)}`);
  }
  return envelope.event;
}

async function fixtureCommand(instance, command) {
  const response = nextMessage(instance.fixture);
  instance.fixture.postMessage(command);
  return response;
}

async function stop(instance) {
  const stopped = fixtureCommand(instance, { kind: 'shutdown' });
  const response = await stopped;
  if (response.kind !== 'stopped') throw new Error('Worker did not acknowledge shutdown');
  instance.worker.terminate();
}

export async function runWorkerFixtureContracts() {
  const module = await WebAssembly.compile(
    await (await fetch('./generated/shoop_audio_worklet.wasm')).arrayBuffer(),
  );

  const explicit = await boot(module, 'explicit');
  await poll(explicit, 11);
  const processed = await fixtureCommand(explicit, {
    kind: 'process',
    frames: 128,
    inputs: [[0.25, ...Array(127).fill(0)]],
    outputChannels: 2,
  });
  if (processed.kind !== 'processed'
      || processed.outputs.length !== 2
      || processed.outputs.some(channel => channel.length !== 128)
      || processed.diagnostics.processedQuanta !== 1) {
    throw new Error(`explicit Worker processing contract failed: ${JSON.stringify(processed)}`);
  }
  await stop(explicit);

  for (const mode of ['cooperative', 'realtime']) {
    const instance = await boot(module, mode);
    await new Promise(resolve => setTimeout(resolve, 40));
    const beforePause = await fixtureCommand(instance, { kind: 'pause' });
    if (beforePause.kind !== 'state' || beforePause.diagnostics.processedQuanta === 0) {
      throw new Error(`${mode} Worker did not progress`);
    }
    await new Promise(resolve => setTimeout(resolve, 25));
    const paused = await fixtureCommand(instance, { kind: 'state' });
    if (paused.diagnostics.processedQuanta !== beforePause.diagnostics.processedQuanta) {
      throw new Error(`${mode} Worker progressed while paused`);
    }
    await fixtureCommand(instance, { kind: 'resume' });
    await new Promise(resolve => setTimeout(resolve, 30));
    const resumed = await fixtureCommand(instance, { kind: 'state' });
    if (resumed.diagnostics.processedQuanta <= paused.diagnostics.processedQuanta) {
      throw new Error(`${mode} Worker did not resume`);
    }
    await poll(instance, mode === 'cooperative' ? 21 : 31);
    await stop(instance);
  }

  const first = await boot(module, 'explicit');
  const second = await boot(module, 'explicit');
  const [firstSnapshot, secondSnapshot] = await Promise.all([poll(first, 41), poll(second, 41)]);
  if (firstSnapshot.callback_count !== 0 || secondSnapshot.callback_count !== 0) {
    throw new Error('fresh Worker instances shared engine progression');
  }
  await fixtureCommand(first, { kind: 'process', frames: 128, outputChannels: 0 });
  const [advanced, isolated] = await Promise.all([poll(first, 42), poll(second, 42)]);
  if (advanced.callback_count !== 1 || isolated.callback_count !== 0) {
    throw new Error('Worker instances leaked engine state');
  }
  await stop(first);
  await stop(second);

  for (let index = 0; index < 3; index += 1) {
    const instance = await boot(module, 'explicit');
    await poll(instance, 50 + index);
    await stop(instance);
  }
  return 'worker fixture contracts: ok';
}
