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
const selfContained = process.env.SELF_CONTAINED === '1';
const outputOnly = process.env.OUTPUT_ONLY === '1';
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
const saturate = process.env.SATURATE === '1';
const selfContainedPath = process.env.SELF_CONTAINED_PATH
  || join(process.cwd(), 'dist', 'shoopdaloop_egui.html');
const profile = await mkdtemp(join(tmpdir(), 'shoopdaloop-egui-chrome-'));
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

async function waitForHttp(url, timeoutMilliseconds) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch (_) {
      // The process is still starting.
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${url}`);
}

async function waitForJson(url, timeoutMilliseconds) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return response.json();
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
  start(chrome, chromeArgs);

  const targets = await waitForJson(`http://${host}:${debugPort}/json`, 60_000);
  const target = targets.find(candidate => candidate.type === 'page');
  if (!target) throw new Error('Chrome exposed no page target');

  websocket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    websocket.onopen = resolve;
    websocket.onerror = reject;
  });

  let nextId = 1;
  const pending = new Map();
  const failures = [];
  websocket.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
      return;
    }
    if (message.method === 'Runtime.exceptionThrown') failures.push(message);
    if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      failures.push(message);
    }
  };

  function call(method, params = {}) {
    const id = nextId++;
    websocket.send(JSON.stringify({ id, method, params }));
    return new Promise(resolve => pending.set(id, resolve));
  }

  async function evaluate(expression) {
    const response = await call('Runtime.evaluate', { expression, returnByValue: true });
    return response.result?.result?.value;
  }

  async function waitFor(predicate, description, timeout = 30_000) {
    const deadline = Date.now() + timeout;
    let state;
    while (Date.now() < deadline) {
      state = await evaluate(statusExpression);
      if (predicate(state)) return state;
      await delay(100);
    }
    throw new Error(`${description}: ${JSON.stringify(state)}`);
  }

  async function clickEnable(id = 'enable_audio') {
    const bounds = await evaluate(`(() => {
      const rect = document.getElementById('${id}').getBoundingClientRect();
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
    })()`);
    await call('Input.dispatchMouseEvent', { type: 'mousePressed', x: bounds.x, y: bounds.y, button: 'left', clickCount: 1 });
    await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x: bounds.x, y: bounds.y, button: 'left', clickCount: 1 });
  }

  await call('Runtime.enable');
  await call('Page.enable');
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
    const query = directFileMicrophone
      ? '?self-test=1'
      : outputOnly
        ? ''
        : '?offline=1&self-test=1';
    entryUrl = `${pathToFileURL(selfContainedPath).href}${query}`;
  } else {
    entryUrl = outputOnly
      ? `${origin}/`
      : `${origin}/?self-test=1${stress ? '&stress=1' : ''}`;
  }
  await call('Page.navigate', { url: entryUrl });

  const statusExpression = `({
    url: location.href,
    title: document.title,
    body: document.body?.textContent?.slice(0, 200),
    status: document.getElementById('runtime_status')?.textContent,
    revision: Number(document.getElementById('runtime_status')?.getAttribute('data-engine-revision')),
    selfTest: document.getElementById('runtime_status')?.getAttribute('data-self-test'),
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
    budgetOverruns: Number(document.getElementById('runtime_status')?.getAttribute('data-callback-budget-overruns')),
    discontinuities: Number(document.getElementById('runtime_status')?.getAttribute('data-render-discontinuities')),
    memoryGrowths: Number(document.getElementById('runtime_status')?.getAttribute('data-memory-growths')),
    overflows: Number(document.getElementById('runtime_status')?.getAttribute('data-command-overflows')),
    webMidi: document.getElementById('runtime_status')?.getAttribute('data-web-midi'),
    waveformSamples: Number(document.getElementById('runtime_status')?.getAttribute('data-waveform-samples')),
    waveformPeak: Number(document.getElementById('runtime_status')?.getAttribute('data-waveform-peak')),
    waveformLoading: document.getElementById('runtime_status')?.getAttribute('data-waveform-loading'),
    enableHidden: document.getElementById('enable_audio')?.hidden,
    outputEnableHidden: document.getElementById('enable_output_audio')?.hidden,
    canvasWidth: document.getElementById('shoop_canvas')?.width,
    canvasHeight: document.getElementById('shoop_canvas')?.height,
  })`;

  if (selfContained && !outputOnly && !directFileMicrophone) {
    const state = await waitFor(
      candidate => candidate.driver === 'Dummy'
        && candidate.revision > 0
        && candidate.selfTest === 'passed',
      'offline dummy session round trip did not finish',
    );
    if (!state.status.includes('Explicit offline dummy')) {
      throw new Error(`offline artifact was not explicit: ${JSON.stringify(state)}`);
    }
    console.log(`explicit self-contained offline dummy passed at ${browserSize}`);
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
    if (state.ownedMediaTracks !== 0 || !state.enableHidden || !state.outputEnableHidden) {
      throw new Error(`output-only mode acquired input or left enable actions visible: ${JSON.stringify(state)}`);
    }
    console.log(`${selfContained ? 'direct-file' : 'hosted'} output-only audio passed at ${browserSize}`);
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
      stress ? 360_000 : 120_000,
    );
    if (!(state.callbacks > 0 && state.frames >= state.callbacks * 128)) {
      throw new Error(`worklet callback evidence is invalid: ${JSON.stringify(state)}`);
    }
    if (stress && state.callbacks < 1_500) {
      throw new Error(`stress recording ended too early: ${JSON.stringify(state)}`);
    }
    if (!(state.inputPeak > 0 && state.outputPeak > 0)) {
      throw new Error(`non-zero I/O evidence is missing: ${JSON.stringify(state)}`);
    }
    if (!(state.sampleRate > 0 && state.quantum === 128 && state.captureChannels > 0)) {
      throw new Error(`context rate/quantum diagnostics are invalid: ${JSON.stringify(state)}`);
    }
    if (!(state.generation > 0 && state.ownedMediaTracks > 0)) {
      throw new Error(`audio generation does not own a live media track: ${JSON.stringify(state)}`);
    }
    if (
      state.budgetOverruns !== 0
      || state.overflows !== 0
      || !Number.isFinite(state.memoryGrowths)
      || state.memoryGrowths > 32
      || state.webMidi !== 'unavailable'
    ) {
      throw new Error(`render, memory-growth, protocol, or Web MIDI diagnostics are invalid: ${JSON.stringify(state)}`);
    }
    if (!(state.canvasWidth > 0 && state.canvasHeight > 0)) {
      throw new Error(`canvas was not sized: ${JSON.stringify(state)}`);
    }
    const firstCallbacks = state.callbacks;
    const firstGeneration = state.generation;
    await evaluate("document.getElementById('enable_audio').click()");
    await delay(250);
    state = await evaluate(statusExpression);
    if (!(state.callbacks > firstCallbacks) || state.generation !== firstGeneration) {
      throw new Error(`repeated start changed the active generation or stopped callbacks: ${JSON.stringify(state)}`);
    }
    if (saturate) {
      const callbacksBeforeSaturation = state.callbacks;
      await evaluate("dispatchEvent(new Event('shoop-test-audio-saturate'))");
      state = await waitFor(
        candidate => candidate.overflows > 0 && candidate.callbacks > callbacksBeforeSaturation,
        'bounded command saturation was not observable or stopped callbacks',
      );
      if (state.driver !== 'Running') {
        throw new Error(`driver did not remain running after bounded saturation: ${JSON.stringify(state)}`);
      }
    }
    if (lifecycle) {
      await evaluate("dispatchEvent(new Event('shoop-test-audio-suspend'))");
      const suspended = await waitFor(candidate => candidate.driver === 'Suspended', 'context suspension was not visible');
      await evaluate("dispatchEvent(new Event('shoop-test-audio-resume'))");
      state = await waitFor(
        candidate => candidate.driver === 'Running' && candidate.callbacks > suspended.callbacks,
        'context did not resume callback progress',
      );
      await evaluate("dispatchEvent(new Event('shoop-test-audio-track-end'))");
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
      await evaluate("dispatchEvent(new Event('shoop-test-audio-fail'))");
      const failed = await waitFor(candidate => candidate.driver === 'Failed', 'worklet failure was not visible');
      if (failed.enableHidden) throw new Error('retry action stayed hidden after worklet failure');
      await clickEnable();
      state = await waitFor(
        candidate => candidate.driver === 'Running' && candidate.callbacks > 0,
        'worklet retry did not create one running generation',
      );
      await evaluate("dispatchEvent(new Event('shoop-test-audio-shutdown'))");
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
