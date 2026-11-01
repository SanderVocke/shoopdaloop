let instance;
let chain = Promise.resolve();
self.onmessage = event => {
    const message = event.data;
    chain = chain.then(async () => {
        if (message.init) {
            const module = message.module || await WebAssembly.compile(
                await (await fetch(message.url)).arrayBuffer(),
            );
            instance = await WebAssembly.instantiate(module, {});
            self.postMessage({ ready: true });
            return;
        }
        const input = new Uint8Array(message.bytes);
        if (!instance || input.length === 0 || input.length > 65536) {
            throw new Error("invalid codec worker input");
        }
        const exports = instance.exports;
        const pointer = exports.codec_input(input.length);
        if (!pointer) throw new Error("codec input reservation refused");
        new Uint8Array(exports.memory.buffer, pointer, input.length).set(input);
        const length = exports.codec_run();
        if (!length || length > 65536) throw new Error("invalid codec worker output");
        const bytes = new Uint8Array(exports.memory.buffer, exports.codec_output(), length).slice();
        self.postMessage({ job: message.job, bytes: bytes.buffer }, [bytes.buffer]);
    }).catch(error => {
        self.postMessage({ job: message.job, error: String(error).slice(0, 1024) });
    });
};
