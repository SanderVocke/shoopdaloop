// Clock import microbenchmark. Node results are a bridge proxy, not AudioWorklet timings.
const bytes = text => [...new TextEncoder().encode(text)];
const name = text => [bytes(text).length, ...bytes(text)];
const section = (id, body) => [id, body.length, ...body];
function moduleBytes(imported) {
  return Uint8Array.from([
    0, 97, 115, 109, 1, 0, 0, 0,
    ...section(1, [1, 96, 0, 1, 124]),
    ...(imported ? section(2, [1, ...name('shoop_profiling'), ...name('now'), 0, 0]) : []),
    ...section(3, [1, 0]),
    ...section(7, [1, ...name('now'), 0, imported ? 1 : 0]),
    ...section(10, imported ? [1, 4, 0, 16, 0, 11] : [1, 11, 0, 68, 0, 0, 0, 0, 0, 0, 0, 0, 11]),
  ]);
}
const direct = () => performance.now() * 1e6;
const imported = new WebAssembly.Instance(new WebAssembly.Module(moduleBytes(true)), {
  shoop_profiling: { now: direct },
}).exports.now;
const constant = new WebAssembly.Instance(new WebAssembly.Module(moduleBytes(false))).exports.now;
const measure = fn => {
  const batches = [];
  let sum = 0;
  for (let batch = 0; batch < 9; batch += 1) {
    const start = performance.now();
    for (let index = 0; index < 1_000_000; index += 1) sum += fn();
    batches.push((performance.now() - start) * 1e6 / 1_000_000);
  }
  if (!Number.isFinite(sum)) throw new Error('invalid benchmark clock');
  return batches;
};
console.log(JSON.stringify({ clock: 'performance.now', realm: 'Node proxy for Wasm import',
  direct_ns: measure(direct), wasm_constant_ns: measure(constant), wasm_import_ns: measure(imported) }));
