import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

if (typeof WebSocket === 'undefined') {
  throw new Error('WebSocket is unavailable; run Node with --experimental-websocket');
}

const host = '127.0.0.1';
const webPort = Number(process.env.WEB_PORT || 8765);
const debugPort = Number(process.env.DEBUG_PORT || 9222);
const chrome = process.env.CHROME_BIN || 'google-chrome';
const browserSize = process.env.BROWSER_SIZE || '900,600';
const protocolTimeout = Number(process.env.CDP_TIMEOUT_MS || 15_000);
const selfContained = process.env.SELF_CONTAINED === '1';
const outputOnly = process.env.OUTPUT_ONLY === '1';
const workerEngine = process.env.WORKER_ENGINE === '1';
const directFileMicrophone = process.env.DIRECT_FILE_MIC === '1';
const denyFirst = process.env.DENY_FIRST === '1';
if (outputOnly && directFileMicrophone) {
  throw new Error('OUTPUT_ONLY and DIRECT_FILE_MIC are mutually exclusive');
}
if (outputOnly && denyFirst) {
  throw new Error('output-only mode does not support DENY_FIRST');
}
if (directFileMicrophone && !selfContained) {
  throw new Error('DIRECT_FILE_MIC requires SELF_CONTAINED=1');
}
const lifecycle = process.env.LIFECYCLE === '1';
const stress = process.env.STRESS === '1';
const settingsOnly = process.env.SETTINGS_ONLY === '1';
const settingsUnavailable = process.env.SETTINGS_UNAVAILABLE === '1';
const webMidi = process.env.WEB_MIDI === '1';
const webMidiDenyFirst = process.env.WEB_MIDI_DENY_FIRST === '1';
const webMidiOpenFail = process.env.WEB_MIDI_OPEN_FAIL === '1';
if ((webMidiDenyFirst || webMidiOpenFail) && !webMidi) {
  throw new Error('Web MIDI failure modes require WEB_MIDI=1');
}
if (settingsUnavailable && !settingsOnly) {
  throw new Error('SETTINGS_UNAVAILABLE requires SETTINGS_ONLY=1');
}
const selfContainedPath = process.env.SELF_CONTAINED_PATH
  || join(process.cwd(), 'dist', 'shoopdaloop.html');
const profile = await mkdtemp(join(tmpdir(), 'shoopdaloop-chrome-'));
const fakeAudio = join(profile, 'fake-microphone.wav');
const children = [];

function start(command, args, options = {}) {
  const child = spawn(command, args, { stdio: 'pipe', ...options });
  children.push(child);
  return child;
}

function delay(milliseconds) {
  return new Promise(resolve => setTimeout(resolve, milliseconds));
}

function withTimeout(promise, timeoutMilliseconds, description) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`timed out waiting for ${description}`)),
      timeoutMilliseconds,
    );
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

async function waitForHttp(url, timeoutMilliseconds) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(5_000) });
      if (response.ok) return;
    } catch (_) {
      // The process is still starting.
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${url}`);
}

async function waitForJson(url, timeoutMilliseconds, accept = () => true) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(5_000) });
      if (response.ok) {
        const value = await response.json();
        if (accept(value)) return value;
      }
    } catch (_) {
      // The process is still starting.
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${url}`);
}

function fakeMicrophoneWav() {
  const sampleRate = 48000;
  const samples = sampleRate * 30;
  const dataBytes = samples * 2;
  const output = Buffer.alloc(44 + dataBytes);
  output.write('RIFF', 0);
  output.writeUInt32LE(36 + dataBytes, 4);
  output.write('WAVEfmt ', 8);
  output.writeUInt32LE(16, 16);
  output.writeUInt16LE(1, 20);
  output.writeUInt16LE(1, 22);
  output.writeUInt32LE(sampleRate, 24);
  output.writeUInt32LE(sampleRate * 2, 28);
  output.writeUInt16LE(2, 32);
  output.writeUInt16LE(16, 34);
  output.write('data', 36);
  output.writeUInt32LE(dataBytes, 40);
  for (let index = 0; index < samples; index += 1) {
    const sample = Math.round(Math.sin(2 * Math.PI * 440 * index / sampleRate) * 12000);
    output.writeInt16LE(sample, 44 + index * 2);
  }
  return output;
}

let websocket;
try {
  await writeFile(fakeAudio, fakeMicrophoneWav());
  if (!selfContained) {
    start('python3', ['-m', 'http.server', String(webPort), '--bind', host], { cwd: 'dist' });
    await waitForHttp(`http://${host}:${webPort}/`, 15_000);
  }
  const chromeArgs = [
    '--headless=new',
    '--no-sandbox',
    '--disable-dev-shm-usage',
    '--disable-gpu-sandbox',
    '--disable-extensions',
    '--disable-component-extensions-with-background-pages',
    '--enable-webgl',
    '--enable-unsafe-swiftshader',
    '--ignore-gpu-blocklist',
    '--autoplay-policy=no-user-gesture-required',
    '--use-fake-device-for-media-stream',
    `--use-file-for-fake-audio-capture=${fakeAudio}`,
    `--window-size=${browserSize}`,
    `--remote-debugging-port=${debugPort}`,
    `--user-data-dir=${profile}`,
    'about:blank',
  ];
  if (!denyFirst) chromeArgs.splice(-1, 0, '--use-fake-ui-for-media-stream');
  if (settingsUnavailable) chromeArgs.splice(-1, 0, '--disable-local-storage');
  start(chrome, chromeArgs);

  const targets = await waitForJson(
    `http://${host}:${debugPort}/json`,
    60_000,
    candidates => candidates.some(candidate => candidate.type === 'page'),
  );
  const target = targets.find(candidate => candidate.type === 'page');
  if (!target) throw new Error('Chrome exposed no page target');

  websocket = new WebSocket(target.webSocketDebuggerUrl);
  await withTimeout(new Promise((resolve, reject) => {
    websocket.onopen = resolve;
    websocket.onerror = reject;
  }), protocolTimeout, 'Chrome DevTools WebSocket connection');

  let nextId = 1;
  const pending = new Map();
  const failures = [];
  websocket.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const request = pending.get(message.id);
      clearTimeout(request.timer);
      request.resolve(message);
      pending.delete(message.id);
      return;
    }
    if (message.method === 'Runtime.exceptionThrown') failures.push(message);
    if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      failures.push(message);
    }
  };
  websocket.onclose = () => {
    for (const request of pending.values()) {
      clearTimeout(request.timer);
      request.reject(new Error('Chrome DevTools WebSocket closed'));
    }
    pending.clear();
  };

  function call(method, params = {}) {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`timed out waiting for Chrome DevTools ${method}`));
      }, protocolTimeout);
      pending.set(id, { resolve, reject, timer });
      try {
        websocket.send(JSON.stringify({ id, method, params }));
      } catch (error) {
        clearTimeout(timer);
        pending.delete(id);
        reject(error);
      }
    });
  }

  async function evaluate(expression) {
    const response = await call('Runtime.evaluate', { expression, returnByValue: true });
    return response.result?.result?.value;
  }

  async function evaluateAwait(expression) {
    const response = await call('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (response.result?.exceptionDetails) {
      throw new Error(`browser evaluation failed: ${JSON.stringify(response.result.exceptionDetails)}`);
    }
    return response.result?.result?.value;
  }

  async function waitFor(predicate, description, timeout = 30_000) {
    const deadline = Date.now() + timeout;
    let state;
    while (Date.now() < deadline) {
      state = await evaluate(statusExpression);
      if (predicate(state)) return state;
      if (state?.driver === 'Failed') {
        throw new Error(`${description}: ${JSON.stringify({ state, failures })}`);
      }
      await delay(100);
    }
    throw new Error(`${description}: ${JSON.stringify(state)}`);
  }

  async function clickEnable(id = 'enable_audio') {
    const bounds = await evaluate(`(() => {
      document.getElementById('browser_permissions_dialog').hidden = false;
      const button = document.getElementById('${id}');
      button.scrollIntoView({ block: 'center', inline: 'center' });
      const rect = button.getBoundingClientRect();
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
    })()`);
    await call('Input.dispatchMouseEvent', { type: 'mousePressed', x: bounds.x, y: bounds.y, button: 'left', clickCount: 1 });
    await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x: bounds.x, y: bounds.y, button: 'left', clickCount: 1 });
    await evaluate("document.getElementById('browser_permissions_dialog').hidden = true");
  }

  await call('Runtime.enable');
  await call('Page.enable');
  if (webMidi) {
    await call('Page.addScriptToEvaluateOnNewDocument', {
      source: `(() => {
        class FakeMIDIPort extends EventTarget {
          constructor(id, name, manufacturer) {
            super();
            this.id = id;
            this.name = name;
            this.manufacturer = manufacturer;
            this.version = '1';
            this.state = 'connected';
            this.connection = 'closed';
            this.openCalls = 0;
            this.closeCalls = 0;
            this.onstatechange = null;
          }
          open() {
            this.openCalls += 1;
            if (this.failOpenDeferred) {
              this.failOpenDeferred = false;
              return new Promise((_, reject) => { this.rejectDeferredOpen = reject; });
            }
            if (this.failOpen) {
              this.failOpen = false;
              return Promise.reject(new DOMException('test open failure', 'InvalidStateError'));
            }
            if (this.connection !== 'open') {
              this.connection = 'open';
              if (!this.suppressOpenStatechange) {
                queueMicrotask(() => this.access?.onstatechange?.(new Event('statechange')));
              }
            }
            return Promise.resolve(this);
          }
          close() {
            this.closeCalls += 1;
            if (this.connection !== 'closed') {
              this.connection = 'closed';
              queueMicrotask(() => this.access?.onstatechange?.(new Event('statechange')));
            }
            return Promise.resolve(this);
          }
        }
        class FakeMIDIInput extends FakeMIDIPort {
          constructor(...args) { super(...args); this.onmidimessage = null; }
          emit(data) {
            this.onmidimessage?.({ data: Uint8Array.from(data), timeStamp: performance.now() });
          }
        }
        class FakeMIDIOutput extends FakeMIDIPort {
          send(data) {
            if (this.failNext) {
              this.failNext = false;
              throw new DOMException('test send failure', 'InvalidStateError');
            }
            window.__shoopWebMidi.sent.push(Array.from(data));
          }
          clear() {}
        }
        class FakeMIDIAccess extends EventTarget {
          constructor(input, output) {
            super();
            this.inputs = new Map([[input.id, input]]);
            this.outputs = new Map([[output.id, output]]);
            input.access = this;
            output.access = this;
            this.sysexEnabled = true;
            this.onstatechange = null;
          }
        }
        Object.defineProperties(globalThis, {
          MIDIPort: { value: FakeMIDIPort, configurable: true },
          MIDIInput: { value: FakeMIDIInput, configurable: true },
          MIDIOutput: { value: FakeMIDIOutput, configurable: true },
          MIDIAccess: { value: FakeMIDIAccess, configurable: true },
        });
        const input = new FakeMIDIInput('test-input', 'APC MINI MIDI', 'Shoop Test');
        const output = new FakeMIDIOutput('test-output', 'APC MINI MIDI', 'Shoop Test');
        input.suppressOpenStatechange = ${webMidiOpenFail};
        output.failOpen = ${webMidiOpenFail};
        const access = new FakeMIDIAccess(input, output);
        window.__shoopWebMidi = {
          access, input, output, sent: [], requested: [], denyNext: ${webMidiDenyFirst}
        };
        Object.defineProperty(navigator, 'requestMIDIAccess', {
          configurable: true,
          value: options => {
            window.__shoopWebMidi.requested.push({ sysex: options?.sysex === true });
            if (window.__shoopWebMidi.denyNext) {
              window.__shoopWebMidi.denyNext = false;
              return Promise.reject(new DOMException('test denial', 'NotAllowedError'));
            }
            return Promise.resolve(access);
          },
        });
      })();`,
    });
  }
  const origin = `http://${host}:${webPort}`;
  if (denyFirst) {
    await call('Browser.setPermission', {
      permission: { name: 'audioCapture' },
      setting: 'denied',
      origin,
    });
  }
  let entryUrl;
  if (selfContained) {
    const query = workerEngine
      ? '?worker=1&self-test=1'
      : settingsOnly
        ? settingsUnavailable
        ? '?offline=1&settings-test=unavailable'
        : '?offline=1&settings-test=write'
      : webMidi
        ? '?web-midi-test=1'
        : directFileMicrophone
          ? '?self-test=1'
          : outputOnly
            ? ''
            : '?offline=1&self-test=1';
    entryUrl = `${pathToFileURL(selfContainedPath).href}${query}`;
  } else {
    entryUrl = settingsOnly
      ? settingsUnavailable
        ? `${origin}/?offline=1&settings-test=unavailable`
        : `${origin}/?offline=1&settings-test=write`
      : workerEngine
        ? `${origin}/?worker=1&self-test=1`
        : outputOnly
          ? `${origin}/`
          : webMidi
            ? `${origin}/?web-midi-test=1`
            : `${origin}/?self-test=1${stress ? '&stress=1' : ''}${browserSize === '360,200' ? '&session-only=1' : ''}`;
  }
  await call('Page.navigate', { url: entryUrl });

  const statusExpression = `({
    url: location.href,
    title: document.title,
    body: document.body?.textContent?.slice(0, 200),
    status: document.getElementById('runtime_status')?.textContent,
    revision: Number(document.getElementById('runtime_status')?.getAttribute('data-engine-revision')),
    selfTest: document.getElementById('runtime_status')?.getAttribute('data-self-test'),
    selfTestError: document.getElementById('runtime_status')?.getAttribute('data-self-test-error'),
    selfTestNonzeroIo: document.getElementById('runtime_status')?.getAttribute('data-self-test-nonzero-io'),
    dryWetForm: document.getElementById('runtime_status')?.getAttribute('data-dry-wet-form'),
    settingsTest: document.getElementById('runtime_status')?.getAttribute('data-settings-self-test'),
    settingsChannels: Number(document.getElementById('runtime_status')?.getAttribute('data-settings-channels')),
    settingsMidi: document.getElementById('runtime_status')?.getAttribute('data-settings-midi'),
    settingsRecovery: document.getElementById('runtime_status')?.getAttribute('data-settings-recovery'),
    driver: document.getElementById('runtime_status')?.getAttribute('data-driver-state'),
    generation: Number(document.getElementById('runtime_status')?.getAttribute('data-audio-generation')),
    ownedMediaTracks: Number(document.getElementById('runtime_status')?.getAttribute('data-owned-media-tracks')),
    callbacks: Number(document.getElementById('runtime_status')?.getAttribute('data-callback-count')),
    frames: Number(document.getElementById('runtime_status')?.getAttribute('data-processed-frames')),
    inputPeak: Number(document.getElementById('runtime_status')?.getAttribute('data-input-peak')),
    outputPeak: Number(document.getElementById('runtime_status')?.getAttribute('data-output-peak')),
    sampleRate: Number(document.getElementById('runtime_status')?.getAttribute('data-sample-rate')),
    captureChannels: Number(document.getElementById('runtime_status')?.getAttribute('data-capture-channels')),
    quantum: Number(document.getElementById('runtime_status')?.getAttribute('data-render-quantum')),
    xruns: Number(document.getElementById('runtime_status')?.getAttribute('data-xruns')),
    discontinuities: Number(document.getElementById('runtime_status')?.getAttribute('data-render-discontinuities')),
    memoryGrowths: Number(document.getElementById('runtime_status')?.getAttribute('data-memory-growths')),
    renderMemoryGrowths: Number(document.getElementById('runtime_status')?.getAttribute('data-render-memory-growths')),
    overflows: Number(document.getElementById('runtime_status')?.getAttribute('data-command-overflows')),
    webMidi: document.getElementById('runtime_status')?.getAttribute('data-web-midi'),
    webMidiEndpoints: Number(document.getElementById('runtime_status')?.getAttribute('data-web-midi-endpoints')),
    webMidiSelfTest: document.getElementById('runtime_status')?.getAttribute('data-web-midi-self-test'),
    webMidiTrackDrops: Number(document.getElementById('runtime_status')?.getAttribute('data-web-midi-track-drops')),
    webMidiTrackRefusals: Number(document.getElementById('runtime_status')?.getAttribute('data-web-midi-track-refusals')),
    webMidiControlRefusals: Number(document.getElementById('runtime_status')?.getAttribute('data-web-midi-control-refusals')),
    webMidiStatus: document.getElementById('enable_midi')?.title,
    webMidiInputConnection: window.__shoopWebMidi?.input.connection,
    webMidiOutputConnection: window.__shoopWebMidi?.output.connection,
    webMidiInputHandler: typeof window.__shoopWebMidi?.input.onmidimessage === 'function',
    applicationPorts: Number(document.getElementById('runtime_status')?.getAttribute('data-application-ports')),
    hostPorts: Number(document.getElementById('runtime_status')?.getAttribute('data-host-ports')),
    confirmedLinks: Number(document.getElementById('runtime_status')?.getAttribute('data-confirmed-links')),
    selectedLoops: Number(document.getElementById('runtime_status')?.getAttribute('data-selected-loops')),
    luaControlPorts: Number(document.getElementById('runtime_status')?.getAttribute('data-lua-control-ports')),
    midiHostPorts: Number(document.getElementById('runtime_status')?.getAttribute('data-midi-host-ports')),
    waveformSamples: Number(document.getElementById('runtime_status')?.getAttribute('data-waveform-samples')),
    waveformPeak: Number(document.getElementById('runtime_status')?.getAttribute('data-waveform-peak')),
    waveformLoading: document.getElementById('runtime_status')?.getAttribute('data-waveform-loading'),
    midiDetailChannels: Number(document.getElementById('runtime_status')?.getAttribute('data-midi-detail-channels')),
    midiDetailEvents: Number(document.getElementById('runtime_status')?.getAttribute('data-midi-detail-events')),
    midiDetailLoading: document.getElementById('runtime_status')?.getAttribute('data-midi-detail-loading'),
    enableHidden: document.getElementById('enable_audio')?.hidden,
    outputEnableHidden: document.getElementById('enable_output_audio')?.hidden,
    canvasWidth: document.getElementById('shoop_canvas')?.width,
    canvasHeight: document.getElementById('shoop_canvas')?.height,
  })`;

  if (settingsOnly && settingsUnavailable) {
    const state = await waitFor(
      candidate => candidate.settingsTest === 'unavailable'
        && candidate.settingsRecovery === 'true'
        && candidate.settingsChannels === 2
        && candidate.settingsMidi === 'false'
        && candidate.luaControlPorts === 0
        && candidate.midiHostPorts === 0,
      'unavailable browser storage was not reported with defaults',
    );
    console.log(`${selfContained ? 'direct-file' : 'hosted'} unavailable browser settings storage passed`);
  } else if (settingsOnly) {
    let state = await waitFor(
      candidate => candidate.settingsTest === 'written'
        && candidate.settingsChannels === 6
        && candidate.settingsMidi === 'true',
      'browser settings write did not finish',
    );
    const settingsKey = 'org.shoopdaloop.egui.settings';
    const stored = await evaluate(`localStorage.getItem('${settingsKey}')`);
    if (!stored?.includes('shoop-egui-settings')) {
      throw new Error(`browser settings text was not persisted: ${stored}`);
    }
    const baseUrl = entryUrl.split('?')[0];
    await call('Page.navigate', { url: `${baseUrl}?offline=1&settings-test=verify` });
    state = await waitFor(
      candidate => candidate.settingsTest === 'passed'
        && candidate.settingsChannels === 6
        && candidate.settingsMidi === 'true'
        && candidate.luaControlPorts === 2
        && candidate.midiHostPorts === 0,
      'browser settings reload did not reach the Add Track consumer',
    );

    const rejectedDocument = JSON.stringify({
      format: 'shoop-egui-settings',
      format_version: { major: 99, minor: 0 },
      document_version: 99,
      writer_version: 'future',
      values: { 'tracks.new.default_audio_channels': 9 },
    });
    await evaluate(`localStorage.setItem('${settingsKey}', ${JSON.stringify(rejectedDocument)})`);
    await call('Page.navigate', { url: `${baseUrl}?offline=1&settings-test=rejected` });
    state = await waitFor(
      candidate => candidate.settingsTest === 'rejected'
        && candidate.settingsRecovery === 'true'
        && candidate.settingsChannels === 2
        && candidate.settingsMidi === 'false'
        && candidate.luaControlPorts === 0
        && candidate.midiHostPorts === 0,
      'future browser settings were not rejected transactionally',
    );
    const retained = await evaluate(`localStorage.getItem('${settingsKey}')`);
    if (retained !== rejectedDocument) {
      throw new Error(`rejected browser settings were overwritten: ${retained}`);
    }

    const invalidDocument = JSON.stringify({
      format: 'shoop-egui-settings',
      format_version: { major: 1, minor: 0 },
      document_version: 1,
      writer_version: 'invalid',
      values: {
        'tracks.new.default_audio_channels': 'wrong-type',
        'tracks.new.default_midi': false,
      },
    });
    await evaluate(`localStorage.setItem('${settingsKey}', ${JSON.stringify(invalidDocument)})`);
    await call('Page.navigate', { url: `${baseUrl}?offline=1&settings-test=invalid` });
    state = await waitFor(
      candidate => candidate.settingsTest === 'invalid'
        && candidate.settingsRecovery === 'false'
        && candidate.settingsChannels === 2
        && candidate.settingsMidi === 'false'
        && candidate.luaControlPorts === 0
        && candidate.midiHostPorts === 0,
      'invalid known browser setting did not default with a diagnostic',
    );

    await evaluate(`localStorage.removeItem('${settingsKey}')`);
    await call('Page.navigate', {
      url: `${baseUrl}?offline=1&settings-test=save-failure&settings-save-failure=1`,
    });
    state = await waitFor(
      candidate => candidate.settingsTest === 'save-failed'
        && candidate.settingsChannels === 2
        && candidate.settingsMidi === 'false'
        && candidate.luaControlPorts === 0
        && candidate.midiHostPorts === 0,
      'failed browser save changed active settings',
    );
    if (await evaluate(`localStorage.getItem('${settingsKey}')`) !== null) {
      throw new Error('failed browser save wrote settings bytes');
    }
    console.log(`${selfContained ? 'direct-file' : 'hosted'} browser settings save/reload/rejection passed`);
  } else if (
    selfContained && !workerEngine && !outputOnly && !directFileMicrophone && !webMidi
  ) {
    const state = await waitFor(
      candidate => candidate.driver === 'Dummy'
        && candidate.revision > 0
        && candidate.selfTest === 'passed',
      'offline dummy session round trip did not finish',
    );
    if (state.driver !== 'Dummy' || !entryUrl.includes('offline=1') || state.dryWetForm !== 'built-in-synth') {
      throw new Error(`offline artifact or dry/wet capability evidence was incomplete: ${JSON.stringify(state)}`);
    }
    console.log(`explicit self-contained offline dummy passed at ${browserSize}`);
  } else if (workerEngine) {
    const state = await waitFor(
      candidate => candidate.driver === 'Dummy'
        && candidate.revision > 0
        && candidate.selfTest === 'passed'
        && candidate.callbacks > 0,
      'browser Worker dummy did not advance and finish the session round trip',
    );
    if (state.ownedMediaTracks !== 0 || !state.enableHidden || !state.outputEnableHidden) {
      throw new Error(`Worker engine unexpectedly owned physical audio presentation: ${JSON.stringify(state)}`);
    }
    if (!selfContained) {
      const fixtureResult = await evaluateAwait(
        "import('./worker_fixture_contract.js').then(module => module.runWorkerFixtureContracts())",
      );
      if (fixtureResult !== 'worker fixture contracts: ok') {
        throw new Error(`Worker fixture contract failed: ${fixtureResult}`);
      }
      const compositionResult = await evaluateAwait(
        "import('./worker_fixture_contract.js').then(module => module.runApplicationCompositionIsolation())",
      );
      if (compositionResult !== 'application composition isolation: ok') {
        throw new Error(`application composition isolation failed: ${compositionResult}`);
      }
    }
    console.log(`browser Worker engine passed at ${browserSize}`);
  } else if (outputOnly) {
    await waitFor(
      candidate => candidate.driver === 'AwaitingGesture' && candidate.revision > 0,
      'output-only enable action was not presented',
    );
    await clickEnable('enable_output_audio');
    const state = await waitFor(
      candidate => candidate.driver === 'Running' && candidate.callbacks > 0,
      'output-only browser audio did not start',
    );
    if (!(state.frames >= state.callbacks * 128 && state.quantum === 128)) {
      throw new Error(`output-only callback evidence is invalid: ${JSON.stringify(state)}`);
    }
    if (state.ownedMediaTracks !== 0 || state.enableHidden || !state.outputEnableHidden) {
      throw new Error(`output-only mode acquired input or hid the microphone upgrade action: ${JSON.stringify(state)}`);
    }
    console.log(`${selfContained ? 'direct-file' : 'hosted'} output-only audio passed at ${browserSize}`);
  } else if (webMidi) {
    await waitFor(
      candidate => candidate.webMidiSelfTest === 'awaiting-permission'
        && candidate.driver === 'AwaitingGesture',
      'Web MIDI test enable actions were not presented',
    );
    await clickEnable('enable_midi');
    if (webMidiDenyFirst) {
      await waitFor(
        candidate => candidate.webMidi === 'Denied',
        'Web MIDI permission denial was not visible',
      );
      await clickEnable('enable_midi');
    }
    if (webMidiOpenFail) {
      await waitFor(
        candidate => candidate.webMidi === 'Running'
          && candidate.webMidiEndpoints === 1
          && candidate.webMidiSelfTest !== 'control-ready-without-audio'
          && candidate.webMidiStatus?.includes('could not open Web MIDI output'),
        'Web MIDI port-open failure was not propagated to inventory and status',
      );
      await evaluate(`(() => {
        window.__shoopWebMidi.input.suppressOpenStatechange = false;
        window.__shoopWebMidi.access.onstatechange?.(new Event('statechange'));
      })()`);
    }
    await waitFor(
      candidate => candidate.webMidi === 'Running'
        && candidate.webMidiEndpoints === 2
        && candidate.webMidiSelfTest === 'control-ready-without-audio'
        && candidate.driver === 'AwaitingGesture'
        && candidate.luaControlPorts === 2,
      'Web MIDI control did not become ready independently of audio',
      120_000,
    );
    await evaluate(`(() => {
      const midi = window.__shoopWebMidi;
      midi.output.connection = 'closed';
      midi.output.failOpenDeferred = true;
      midi.access.onstatechange?.(new Event('statechange'));
    })()`);
    const deferredOpenDeadline = Date.now() + 10_000;
    while (Date.now() < deferredOpenDeadline) {
      if (await evaluate(`typeof window.__shoopWebMidi.output.rejectDeferredOpen === 'function'`)) {
        break;
      }
      await delay(25);
    }
    if (!(await evaluate(`typeof window.__shoopWebMidi.output.rejectDeferredOpen === 'function'`))) {
      throw new Error('Web MIDI deferred stale-open fixture was not reached');
    }
    await evaluate(`(() => {
      const midi = window.__shoopWebMidi;
      const reject = midi.output.rejectDeferredOpen;
      midi.output.rejectDeferredOpen = null;
      midi.access.onstatechange?.(new Event('statechange'));
      queueMicrotask(() => reject(new DOMException('test stale open failure', 'InvalidStateError')));
    })()`);
    await waitFor(
      candidate => candidate.webMidiEndpoints === 2
        && candidate.webMidiOutputConnection === 'open'
        && candidate.webMidiStatus?.includes('superseded endpoint generation'),
      'stale Web MIDI open failure mutated current endpoint truth or lacked diagnostics',
    );

    const controlDeadline = Date.now() + 10_000;
    let preAudioControlOutput = [];
    while (Date.now() < controlDeadline) {
      preAudioControlOutput = await evaluate('window.__shoopWebMidi.sent');
      if (preAudioControlOutput.length > 0) break;
      await delay(50);
    }
    if (preAudioControlOutput.length === 0) {
      throw new Error('Web MIDI control output did not run before audio startup');
    }
    await clickEnable();
    const midiState = await waitFor(
      candidate => candidate.webMidi === 'Running'
        && candidate.webMidiEndpoints === 2
        && candidate.midiHostPorts === 2
        && candidate.webMidiSelfTest === 'awaiting-input'
        && candidate.webMidiInputConnection === 'open'
        && candidate.webMidiInputHandler
        && candidate.driver === 'Running',
      'Web MIDI track and control routes were not prepared',
      120_000,
    );
    const request = await evaluate('window.__shoopWebMidi.requested[0]');
    if (!request?.sysex) {
      throw new Error(`Web MIDI SysEx permission was not explicit: ${JSON.stringify(request)}`);
    }
    await evaluate(`(() => {
      const input = window.__shoopWebMidi.input;
      input.emit([0x90, 98, 0x7f]);
      input.emit([0x90, 83, 0x7f]);
      input.emit([0x80, 83, 0x40]);
      input.emit([0x80, 98, 0x40]);
    })()`);
    await waitFor(
      candidate => candidate.webMidiSelfTest === 'ready-for-playback',
      'Web MIDI input was not consumed by track recording and APC control',
      120_000,
    );
    const prePlaybackOutput = await evaluate('window.__shoopWebMidi.sent');
    if (!prePlaybackOutput.some(message => message.length === 3 && message[2] <= 6)) {
      throw new Error(`APC control output was not delivered: ${JSON.stringify(prePlaybackOutput)}`);
    }
    await evaluate(`(() => {
      window.__shoopWebMidi.sent = [];
      document.getElementById('runtime_status').setAttribute('data-web-midi-playback-ready', 'true');
    })()`);
    await waitFor(
      candidate => candidate.webMidiSelfTest === 'awaiting-playback-output',
      'Web MIDI playback did not start after browser acknowledgement',
    );
    const state = await waitFor(
      candidate => candidate.webMidiSelfTest === 'passed'
        && candidate.callbacks > midiState.callbacks,
      'recorded Web MIDI was not played back',
      120_000,
    );
    let playbackOutput;
    const playbackDeadline = Date.now() + 10_000;
    while (Date.now() < playbackDeadline) {
      playbackOutput = await evaluate('window.__shoopWebMidi.sent');
      const playedNoteOn = playbackOutput.some(message => message.join(',') === '144,83,127');
      const playedNoteOff = playbackOutput.some(message => message.join(',') === '128,83,64');
      if (playedNoteOn && playedNoteOff) break;
      await delay(50);
    }
    const playedNoteOnIndex = playbackOutput.findIndex(message => message.join(',') === '144,83,127');
    const playedNoteOffIndex = playbackOutput.findIndex(message => message.join(',') === '128,83,64');
    if (playedNoteOnIndex < 0 || playedNoteOffIndex <= playedNoteOnIndex) {
      throw new Error(`exact recorded Web MIDI note pair was not replayed in order: ${JSON.stringify(playbackOutput)}`);
    }
    if (!(state.confirmedLinks >= 4 && state.luaControlPorts === 2 && state.midiHostPorts === 2)) {
      throw new Error(`Web MIDI normalized route truth is incomplete: ${JSON.stringify(state)}`);
    }
    if (!(state.callbacks > 0 && state.frames >= state.callbacks * 128 && state.overflows === 0)) {
      throw new Error(`Web MIDI workflow disrupted bounded audio progress: ${JSON.stringify(state)}`);
    }

    await evaluate(`(() => {
      window.__shoopWebMidi.output.failNext = true;
      window.__shoopWebMidi.input.emit([0xf8]);
    })()`);
    await waitFor(
      candidate => candidate.webMidiStatus?.includes('could not send Web MIDI'),
      'Web MIDI output-send failure was not user-visible',
    );

    const callbacksBeforeOverflow = state.callbacks;
    await evaluate(`(() => {
      window.__shoopWebMidi.input.emit([]);
      window.__shoopWebMidi.input.emit(new Array(257).fill(1));
      for (let index = 0; index < 1100; index += 1) {
        window.__shoopWebMidi.input.emit([0xf8]);
      }
    })()`);
    await waitFor(
      candidate => candidate.webMidiTrackDrops > 0
        && candidate.webMidiTrackRefusals >= 2
        && candidate.webMidiControlRefusals >= 2
        && candidate.callbacks > callbacksBeforeOverflow,
      'bounded Web MIDI refusal/drop counters or callback recovery were missing',
    );

    await evaluate(`(() => {
      const midi = window.__shoopWebMidi;
      midi.input.connection = 'closed';
      midi.output.connection = 'closed';
      midi.access.onstatechange?.(new Event('statechange'));
    })()`);
    await waitFor(
      candidate => candidate.webMidiEndpoints === 2
        && candidate.midiHostPorts === 2
        && candidate.webMidiInputConnection === 'open'
        && candidate.webMidiOutputConnection === 'open',
      'Web MIDI connection-close recovery did not reopen stable endpoints',
    );

    await evaluate(`(() => {
      const midi = window.__shoopWebMidi;
      midi.input.state = 'disconnected';
      midi.output.state = 'disconnected';
      midi.access.inputs.delete(midi.input.id);
      midi.access.outputs.delete(midi.output.id);
      midi.access.onstatechange?.(new Event('statechange'));
    })()`);
    await waitFor(
      candidate => candidate.webMidiEndpoints === 0 && candidate.midiHostPorts === 0,
      'Web MIDI hot-unplug was not published',
    );
    await evaluate(`(() => {
      const midi = window.__shoopWebMidi;
      midi.input.state = 'connected';
      midi.output.state = 'connected';
      midi.access.inputs.set(midi.input.id, midi.input);
      midi.access.outputs.set(midi.output.id, midi.output);
      midi.access.onstatechange?.(new Event('statechange'));
    })()`);
    let recovered = await waitFor(
      candidate => candidate.webMidiEndpoints === 2
        && candidate.midiHostPorts === 2
        && candidate.confirmedLinks >= 6,
      'Web MIDI hotplug routes did not reconnect',
      120_000,
    );

    const generationBeforeRestart = recovered.generation;
    const callbacksBeforeRestart = recovered.callbacks;
    await evaluate("shoopAudioDiagnostics.fail(new Event('diagnostic'))");
    await waitFor(
      candidate => candidate.driver === 'Failed',
      'forced worklet failure was not visible during Web MIDI use',
    );
    await clickEnable('enable_output_audio');
    await delay(250);
    recovered = await waitFor(
      candidate => candidate.driver === 'Running'
        && candidate.generation > generationBeforeRestart
        && candidate.callbacks > 0
        && candidate.confirmedLinks >= 6,
      'Web MIDI routes did not recover after worklet restart',
      120_000,
    );
    if (callbacksBeforeRestart === 0 || recovered.overflows === 0) {
      throw new Error(`Web MIDI restart/refusal evidence is invalid: ${JSON.stringify(recovered)}`);
    }
    if (!(recovered.midiDetailChannels > 0 && recovered.midiDetailEvents >= 2) || recovered.midiDetailLoading !== 'false') {
      throw new Error(`MIDI details did not recover after worklet restart: ${JSON.stringify(recovered)}`);
    }
    const portLifecycle = await evaluate(`({
      inputOpen: window.__shoopWebMidi.input.openCalls,
      outputOpen: window.__shoopWebMidi.output.openCalls,
      inputClose: window.__shoopWebMidi.input.closeCalls,
      outputClose: window.__shoopWebMidi.output.closeCalls,
    })`);
    const expectedOutputOpen = webMidiOpenFail ? 6 : 5;
    if (
      portLifecycle.inputOpen !== 3
      || portLifecycle.outputOpen !== expectedOutputOpen
      || portLifecycle.inputClose < 1
      || portLifecycle.outputClose < 1
    ) {
      throw new Error(`Web MIDI port lifecycle churned or failed cleanup: ${JSON.stringify(portLifecycle)}`);
    }
    console.log(`${selfContained ? 'self-contained' : 'hosted'} Web MIDI track/control/hotplug/restart workflow passed: ${JSON.stringify(recovered)}`);
  } else {
    await waitFor(candidate => candidate.selfTest === 'awaiting-audio', 'enable action was not presented');
    await clickEnable();
    if (denyFirst) {
      const denied = await waitFor(candidate => candidate.driver === 'Denied', 'permission denial was not visible');
      if (denied.enableHidden) throw new Error('retry action stayed hidden after denial');
      await call('Browser.resetPermissions');
      await call('Browser.grantPermissions', {
        permissions: ['audioCapture'],
        origin,
      });
      await clickEnable();
    }
    let state = await waitFor(
      candidate => candidate.selfTest === 'passed' && candidate.driver === 'Running',
      'browser physical-audio self-test did not finish',
      stress ? 360_000 : 240_000,
    );
    if (!(state.callbacks > 0 && state.frames >= state.callbacks * 128)) {
      throw new Error(`worklet callback evidence is invalid: ${JSON.stringify(state)}`);
    }
    if (stress && state.callbacks < 1_500) {
      throw new Error(`stress recording ended too early: ${JSON.stringify(state)}`);
    }
    if (state.selfTestNonzeroIo !== 'true') {
      throw new Error(`non-zero I/O evidence is missing: ${JSON.stringify(state)}`);
    }
    if (state.dryWetForm !== 'built-in-synth') {
      throw new Error(`browser Built-in Synth capability evidence is missing: ${JSON.stringify(state)}`);
    }
    if (!(state.sampleRate > 0 && state.quantum === 128 && state.captureChannels > 0)) {
      throw new Error(`context rate/quantum diagnostics are invalid: ${JSON.stringify(state)}`);
    }
    if (!(state.generation > 0 && state.ownedMediaTracks > 0)) {
      throw new Error(`audio generation does not own a live media track: ${JSON.stringify(state)}`);
    }
    if (
      state.xruns !== 0
      || state.overflows !== 0
      || !Number.isFinite(state.memoryGrowths)
      || state.memoryGrowths > 32
      || !Number.isFinite(state.renderMemoryGrowths)
      || state.renderMemoryGrowths > 32
      || state.webMidi !== 'AwaitingGesture'
    ) {
      throw new Error(`render, memory-growth, protocol, or Web MIDI diagnostics are invalid: ${JSON.stringify(state)}`);
    }
    if (!(state.canvasWidth > 0 && state.canvasHeight > 0)) {
      throw new Error(`canvas was not sized: ${JSON.stringify(state)}`);
    }
    if (!(state.applicationPorts > 0 && state.hostPorts >= 4 && state.confirmedLinks > 0)) {
      throw new Error(`normalized browser port truth is missing: ${JSON.stringify(state)}`);
    }
    if (!(state.midiDetailChannels > 0 && state.midiDetailEvents > 16) || state.midiDetailLoading !== 'false') {
      throw new Error(`selected-loop MIDI details are missing or incomplete: ${JSON.stringify(state)}`);
    }
    await evaluate("document.getElementById('shoop_canvas').focus()");
    await call('Input.dispatchKeyEvent', {
      type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27,
    });
    await call('Input.dispatchKeyEvent', {
      type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27,
    });
    await waitFor(candidate => candidate.selectedLoops === 0, 'keyboard script did not clear selection');
    await call('Input.dispatchKeyEvent', {
      type: 'keyDown', key: 'ArrowDown', code: 'ArrowDown', windowsVirtualKeyCode: 40,
    });
    await call('Input.dispatchKeyEvent', {
      type: 'keyUp', key: 'ArrowDown', code: 'ArrowDown', windowsVirtualKeyCode: 40,
    });
    state = await waitFor(
      candidate => candidate.selectedLoops > 0,
      'browser key event did not drive authoritative keyboard.lua selection',
    );
    const firstCallbacks = state.callbacks;
    const firstGeneration = state.generation;
    await evaluate("document.getElementById('enable_audio').click()");
    await delay(250);
    state = await evaluate(statusExpression);
    if (!(state.callbacks > firstCallbacks) || state.generation !== firstGeneration) {
      throw new Error(`repeated start changed the active generation or stopped callbacks: ${JSON.stringify(state)}`);
    }
    if (lifecycle) {
      await evaluate("shoopAudioDiagnostics.suspend(new Event('diagnostic'))");
      const suspended = await waitFor(candidate => candidate.driver === 'Suspended', 'context suspension was not visible');
      await evaluate("shoopAudioDiagnostics.resume(new Event('diagnostic'))");
      state = await waitFor(
        candidate => candidate.driver === 'Running' && candidate.callbacks > suspended.callbacks,
        'context did not resume callback progress',
      );
      await evaluate("shoopAudioDiagnostics.endTrack(new Event('diagnostic'))");
      const trackEnded = await waitFor(
        candidate => candidate.driver === 'Failed' && candidate.ownedMediaTracks === 0,
        'media-track end did not fail visibly and release graph ownership',
      );
      if (trackEnded.enableHidden) throw new Error('retry action stayed hidden after media-track end');
      await clickEnable();
      state = await waitFor(
        candidate => candidate.driver === 'Running' && candidate.callbacks > 0 && candidate.ownedMediaTracks > 0,
        'media-track retry did not create one running generation',
      );
      await evaluate("shoopAudioDiagnostics.fail(new Event('diagnostic'))");
      const failed = await waitFor(candidate => candidate.driver === 'Failed', 'worklet failure was not visible');
      if (failed.enableHidden) throw new Error('retry action stayed hidden after worklet failure');
      await clickEnable();
      state = await waitFor(
        candidate => candidate.driver === 'Running' && candidate.callbacks > 0,
        'worklet retry did not create one running generation',
      );
      await evaluate("shoopAudioDiagnostics.shutdown(new Event('diagnostic'))");
      const stopped = await waitFor(candidate => candidate.driver === 'Stopped', 'audio shutdown was not visible');
      const stoppedCallbacks = stopped.callbacks;
      await delay(250);
      state = await evaluate(statusExpression);
      if (state.callbacks !== stoppedCallbacks || state.enableHidden || state.ownedMediaTracks !== 0) {
        throw new Error(`shutdown did not stop callbacks, release media, and expose retry: ${JSON.stringify(state)}`);
      }
    }
    console.log(`${selfContained ? 'direct-file' : 'hosted'} browser Web Audio self-test passed at ${browserSize}, callback ${state.callbacks}`);
  }
  if (failures.length > 0) {
    throw new Error(`browser reported runtime errors: ${JSON.stringify(failures)}`);
  }
} finally {
  if (websocket) websocket.close();
  for (const child of children.reverse()) child.kill('SIGTERM');
  await Promise.all(children.map(child => new Promise(resolve => {
    if (child.exitCode !== null || child.signalCode !== null) {
      resolve();
    } else {
      child.once('exit', resolve);
      setTimeout(resolve, 2_000);
    }
  })));
  await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}
