#!/usr/bin/env node

import {copyFile, mkdir, readdir, rm} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import path from 'node:path';

const chrome = process.env.CHROME_BIN || 'google-chrome';
const webPort = Number(process.env.WEB_PORT || 19890);
const debugPort = Number(process.env.DEBUG_PORT || 19891);
const root = path.resolve('../../..');
const downloads = path.join(root, 'target/browser-tracing-smoke');
const realm = process.env.TRACE_REALM || 'worker';
const output = path.join(root, `target/perfetto-validation/browser-${realm}.pftrace`);
const processes = [];

function start(command, args, options = {}) {
  const child = spawn(command, args, {stdio: 'ignore', ...options});
  processes.push(child);
  return child;
}

async function waitForHttp(url, attempts = 300) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for ${url}`);
}

async function waitForJson(url, attempts = 300) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return response.json();
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for ${url}`);
}

let socket;
try {
  await rm(downloads, {recursive: true, force: true});
  await mkdir(downloads, {recursive: true});
  await mkdir(path.dirname(output), {recursive: true});
  start('python3', [
    path.join(root, 'scripts/serve_web.py'),
    'dist', '--port', String(webPort),
  ], {cwd: path.resolve('.')});
  await waitForHttp(`http://127.0.0.1:${webPort}/`, 100);
  start(chrome, [
    '--headless=new', '--no-sandbox', '--disable-dev-shm-usage',
    '--disable-gpu-sandbox', '--enable-webgl', '--enable-unsafe-swiftshader',
    '--ignore-gpu-blocklist', '--autoplay-policy=no-user-gesture-required',
    '--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream',
    `--remote-debugging-port=${debugPort}`,
    `--user-data-dir=${path.join(downloads, 'profile')}`,
    `http://127.0.0.1:${webPort}/?${realm === 'worker' ? 'worker=1&' : ''}tracing=1&tracing-smoke-test=1`,
  ], {env: {...process.env, HOME: process.env.HOME}});
  const targets = await waitForJson(`http://127.0.0.1:${debugPort}/json/list`);
  const page = targets.find(target => target.type === 'page');
  if (!page) throw new Error('Chrome exposed no page target');
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, {once: true});
    socket.addEventListener('error', reject, {once: true});
  });
  const consoleMessages = [];
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.method === 'Runtime.consoleAPICalled') {
      consoleMessages.push(message.params.args.map(arg => arg.value || arg.description).join(' '));
    }
    if (message.method === 'Runtime.exceptionThrown') {
      consoleMessages.push(message.params.exceptionDetails.text);
    }
  });
  let nextId = 0;
  function command(method, params = {}) {
    const id = ++nextId;
    return new Promise((resolve, reject) => {
      const listener = event => {
        const message = JSON.parse(event.data);
        if (message.id !== id) return;
        socket.removeEventListener('message', listener);
        if (message.error) reject(new Error(JSON.stringify(message.error)));
        else resolve(message.result);
      };
      socket.addEventListener('message', listener);
      socket.send(JSON.stringify({id, method, params}));
    });
  }
  await command('Runtime.enable');
  await command('Browser.setDownloadBehavior', {
    behavior: 'allow',
    downloadPath: downloads,
    eventsEnabled: true,
  });
  if (realm === 'audio') {
    for (let attempt = 0; attempt < 300; attempt += 1) {
      try {
        const result = await command('Runtime.evaluate', {
          expression: `(() => {
            const button = document.getElementById('enable_output_audio');
            if (!button || button.disabled || button.hidden || typeof button.onclick !== 'function') return false;
            button.click();
            return true;
          })()`,
          returnByValue: true,
        });
        if (result.result.value) break;
      } catch {}
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    if (process.env.TRACE_REBUILD === '1') {
      await new Promise(resolve => setTimeout(resolve, 1000));
      const upgrade = await command('Runtime.evaluate', {
        expression: `(() => {
          const button = document.getElementById('enable_audio');
          if (!button || button.disabled || button.hidden || typeof button.onclick !== 'function') return false;
          button.click();
          return true;
        })()`,
        returnByValue: true,
      });
      if (!upgrade.result.value) throw new Error('could not request traced AudioWorklet rebuild');
    }
  }
  let saved = null;
  for (let attempt = 0; attempt < 900; attempt += 1) {
    try {
      const result = await command('Runtime.evaluate', {
        expression: 'document.body?.getAttribute("data-perfetto-trace-saved")',
        returnByValue: true,
      });
      saved = result.result.value;
      if (saved) break;
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  if (!saved) {
    const diagnostic = await command('Runtime.evaluate', {
      expression: `JSON.stringify({
        title: document.title,
        body: document.body?.innerText,
        status: document.getElementById('browser_status')?.textContent,
        saved: document.body?.getAttribute('data-perfetto-trace-saved')
      })`,
      returnByValue: true,
    });
    throw new Error(
      `browser trace was not finalized: ${diagnostic.result.value}; console=${JSON.stringify(consoleMessages)}`,
    );
  }
  let trace;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const files = await readdir(downloads);
    trace = files.find(name => name.endsWith('.pftrace'));
    if (trace) break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  if (!trace) throw new Error('browser trace download is missing');
  await copyFile(path.join(downloads, trace), output);
  console.log(`browser ${realm} Perfetto capture: ${output}`);
} finally {
  socket?.close();
  for (const child of processes.reverse()) child.kill('SIGTERM');
}
