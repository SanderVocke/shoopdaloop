import { parentPort, workerData } from 'node:worker_threads';

if (!parentPort || !workerData?.workerModuleUrl) {
  throw new Error('Shoop Node Worker bootstrap requires a parent port and Worker module URL');
}

globalThis.self = globalThis;
self.close = () => parentPort.close();
const buffered = [];
let imported = false;
parentPort.on('message', data => {
  if (imported) self.onmessage?.({ data });
  else buffered.push(data);
});

await import(workerData.workerModuleUrl);
imported = true;
for (const data of buffered.splice(0)) self.onmessage?.({ data });
parentPort.postMessage({ kind: 'node-bootstrap-ready' });
