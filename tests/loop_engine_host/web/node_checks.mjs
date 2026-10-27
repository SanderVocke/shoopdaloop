import { readFileSync } from "node:fs";
import { initSync, BrowserClient, BrowserBridge } from "./pkg/fixture.js";
import { checkBindings } from "./bindings_checks.js";
const module = new WebAssembly.Module(readFileSync(new URL("./pkg/fixture_bg.wasm", import.meta.url)));
const wasm = initSync({ module });
console.log(JSON.stringify({ ...checkBindings(wasm, BrowserClient, BrowserBridge), node: process.version }));
