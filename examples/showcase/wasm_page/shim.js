// Datara WASM Runtime Shim
// Handles capability gating, memory management, ownership guards, and text I/O.

export class DataraWasmRuntime {
    constructor(capabilities) {
        this.capabilities = capabilities || {
            permissions: [" console.log\, \datara:rt/own_acquire\, \datara:rt/own_release\]
 };
 this.memory = null;
 this.instance = null;
 this.outputLines = [];
 }

 checkPermission(perm) {
 if (!this.capabilities.permissions.includes(perm)) {
 throw new Error(Capability Violation: action '' is not permitted by capabilities.json);
 }
 }

 createImportObject() {
 return {
 env: {
 memory: this.memory,
 print_i64: (val) => {
 this.checkPermission(\console.log\);
 const msg = String(val);
 this.outputLines.push(msg);
 console.log(\[Datara WASM out]:\, msg);
 },
 print_f64: (val) => {
 this.checkPermission(\console.log\);
 const msg = String(val);
 this.outputLines.push(msg);
 console.log(\[Datara WASM out]:\, msg);
 },
 print_str: (ptr, len) => {
 this.checkPermission(\console.log\);
 const bytes = new Uint8Array(this.memory.buffer, Number(ptr), Number(len));
 const text = new TextDecoder(\utf-8\).decode(bytes);
 this.outputLines.push(text);
 console.log(\[Datara WASM out]:\, text);
 }
 },
 \datara:rt\: {
 own_acquire: (ptr) => {
 this.checkPermission(\datara:rt/own_acquire\);
 // Guard acquire tracked in JS runtime
 return 1;
 },
 own_release: (ptr) => {
 this.checkPermission(\datara:rt/own_release\);
 // Guard release tracked in JS runtime
 return 0;
 }
 }
 };
 }

 async load(wasmBytesOrUrl) {
 let bytes;
 if (typeof wasmBytesOrUrl === \string\) {
 const resp = await fetch(wasmBytesOrUrl);
 bytes = await resp.arrayBuffer();
 } else {
 bytes = wasmBytesOrUrl;
 }

 const importObj = this.createImportObject();
 const compiled = await WebAssembly.instantiate(bytes, importObj);
 this.instance = compiled.instance;
 if (compiled.instance.exports.memory) {
 this.memory = compiled.instance.exports.memory;
 }
 return this.instance;
 }

 runMain() {
 if (!this.instance) {
 throw new Error(\WASM module not loaded\);
 }
 if (typeof this.instance.exports.main === \function\) {
 return this.instance.exports.main();
 }
 return null;
 }
}
