import { spawn } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

if (typeof WebSocket === 'undefined') {
  throw new Error('WebSocket is unavailable; run Node with --experimental-websocket');
}

const host = '127.0.0.1';
const webPort = 8765;
const debugPort = 9222;
const chrome = process.env.CHROME_BIN || 'google-chrome';
const browserSize = process.env.BROWSER_SIZE || '900,600';
const profile = await mkdtemp(join(tmpdir(), 'shoopdaloop-egui-chrome-'));
const children = [];

function start(command, args, options = {}) {
  const child = spawn(command, args, { stdio: 'pipe', ...options });
  children.push(child);
  return child;
}

function delay(milliseconds) {
  return new Promise(resolve => setTimeout(resolve, milliseconds));
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

let websocket;
try {
  start('python3', ['-m', 'http.server', String(webPort), '--bind', host], { cwd: 'dist' });
  start(chrome, [
    '--headless=new',
    '--no-sandbox',
    '--disable-gpu-sandbox',
    '--enable-webgl',
    '--enable-unsafe-swiftshader',
    '--ignore-gpu-blocklist',
    `--window-size=${browserSize}`,
    `--remote-debugging-port=${debugPort}`,
    `--user-data-dir=${profile}`,
    'about:blank',
  ]);

  const targets = await waitForJson(`http://${host}:${debugPort}/json`, 15_000);
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
    if (
      message.method === 'Runtime.consoleAPICalled'
      && message.params.type === 'error'
    ) failures.push(message);
  };

  function call(method, params = {}) {
    const id = nextId++;
    websocket.send(JSON.stringify({ id, method, params }));
    return new Promise(resolve => pending.set(id, resolve));
  }

  await call('Runtime.enable');
  await call('Page.enable');
  await call('Page.navigate', { url: `http://${host}:${webPort}/?self-test=1` });

  const expression = `({
    status: document.getElementById('runtime_status')?.textContent,
    revision: Number(document.getElementById('runtime_status')?.getAttribute('data-engine-revision')),
    selfTest: document.getElementById('runtime_status')?.getAttribute('data-self-test'),
    canvasWidth: document.getElementById('shoop_canvas')?.width,
    canvasHeight: document.getElementById('shoop_canvas')?.height,
  })`;
  const deadline = Date.now() + 20_000;
  let state;
  while (Date.now() < deadline) {
    const response = await call('Runtime.evaluate', { expression, returnByValue: true });
    state = response.result?.result?.value;
    if (state?.selfTest === 'passed' && state.revision > 0) break;
    if (state?.selfTest === 'failed') throw new Error('browser self-test reported failure');
    await delay(100);
  }
  if (state?.selfTest !== 'passed') {
    throw new Error(`browser self-test did not finish: ${JSON.stringify(state)}`);
  }
  if (!(state.canvasWidth > 0 && state.canvasHeight > 0)) {
    throw new Error(`canvas was not sized: ${JSON.stringify(state)}`);
  }

  const firstRevision = state.revision;
  await delay(250);
  const later = await call('Runtime.evaluate', { expression, returnByValue: true });
  state = later.result?.result?.value;
  if (!(state.revision > firstRevision)) {
    throw new Error(`application revisions stopped advancing: ${JSON.stringify(state)}`);
  }
  if (failures.length > 0) {
    throw new Error(`browser reported runtime errors: ${JSON.stringify(failures)}`);
  }

  console.log(
    `browser dummy-engine self-test passed at ${browserSize} and revision ${state.revision}`,
  );
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
