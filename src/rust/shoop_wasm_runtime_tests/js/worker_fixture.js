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
  const workerSource = (await workerResponse.text()).replace(
    "'./raw_wasm_host.js'",
    JSON.stringify(hostUrl),
  );
  const workerUrl = URL.createObjectURL(new Blob([workerSource], { type: 'text/javascript' }));
  return {
    Worker,
    MessageChannel,
    workerUrl,
    workerOptions: { type: 'module' },
    module: WebAssembly.compile(await wasmResponse.arrayBuffer()),
    cleanup() {
      URL.revokeObjectURL(workerUrl);
      URL.revokeObjectURL(hostUrl);
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
  return { worker, application: application.port1, fixture: fixture.port1 };
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

async function stop(api, instance) {
  const exited = api.waitForExit?.(instance.worker);
  const stopped = await fixtureCommand(instance, { kind: 'shutdown' });
  if (stopped.kind !== 'stopped') throw new Error('fixture shutdown was not acknowledged');
  instance.application.close();
  instance.fixture.close();
  if (exited) await exited;
  else instance.worker.terminate();
}

export async function runProductionWorkerProbe(
  runtime,
  assetLocation,
  protocolVersion,
  commandMaxBytes,
) {
  const api = await runtimeApi(runtime, assetLocation);
  const instances = [];
  try {
    const first = await boot(api, protocolVersion, commandMaxBytes);
    instances.push(first);
    const second = await boot(api, protocolVersion, commandMaxBytes);
    instances.push(second);
    const [firstInitial, secondInitial] = await Promise.all([
      poll(first, protocolVersion, 1),
      poll(second, protocolVersion, 1),
    ]);
    if (firstInitial.callback_count !== 0 || secondInitial.callback_count !== 0) {
      throw new Error('fresh production Workers shared progression');
    }
    const processed = await fixtureCommand(first, {
      kind: 'process',
      frames: 128,
      inputs: [[0.25, ...Array(127).fill(0)]],
      outputChannels: 2,
    });
    if (processed.kind !== 'processed'
        || processed.outputs.length !== 2
        || processed.diagnostics.processedQuanta !== 1) {
      throw new Error(`explicit production processing failed: ${JSON.stringify(processed)}`);
    }
    const [advanced, isolated] = await Promise.all([
      poll(first, protocolVersion, 2),
      poll(second, protocolVersion, 2),
    ]);
    if (advanced.callback_count !== 1 || isolated.callback_count !== 0) {
      throw new Error('production Worker instances leaked engine state');
    }
    await Promise.all(instances.splice(0).map(instance => stop(api, instance)));
    return 'production Worker probe: ok';
  } finally {
    await Promise.all(instances.map(instance => stop(api, instance).catch(() => {})));
    api.cleanup();
  }
}
