use forgen::codegen::wasm::WasmEmitter;
use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn create_sandbox(prefix: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("zta_{}_{}_{}", prefix, std::process::id(), id));
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn run_wasm_node(wasm_path: &Path) -> Result<String, String> {
    let script = r#"
const fs = require('fs');
const wasmBytes = fs.readFileSync('__WASM_PATH__');

let captured = '';
let memoryInstance = null;
const listMap = new Map();
let nextH = 10000n;

function readStr(p) {
    if (!memoryInstance || p < 1024 || p >= memoryInstance.buffer.byteLength - 4) return null;
    const view = new DataView(memoryInstance.buffer);
    const len = view.getUint32(p, true);
    if (len > 0 && len < 4096 && p + 4 + len <= memoryInstance.buffer.byteLength) {
        const bytes = new Uint8Array(memoryInstance.buffer, p + 4, len);
        for (let i = 0; i < bytes.length; i++) {
            const b = bytes[i];
            if (b < 32 && b !== 10 && b !== 13 && b !== 9) return null;
        }
        return new TextDecoder('utf-8').decode(bytes);
    }
    return null;
}

const importObject = {
    "datara:rt": {
        alloc: (sz) => 65536n,
        print: (v) => {
            if (typeof v === 'bigint') {
                const n = Number(v);
                if (n >= 1024 && n < 65536) {
                    const s = readStr(n);
                    if (s !== null) {
                        captured += s + '\n';
                        return;
                    }
                }
            }
            captured += v.toString() + '\n';
        },
        err: (v) => {},
        own_acquire: (p) => p,
        own_release: (p) => {},
        list_create: (cap) => { const h = nextH++; listMap.set(h, []); return h; },
        list_create_1: (a) => { const h = nextH++; listMap.set(h, [a]); return h; },
        list_create_2: (a,b) => { const h = nextH++; listMap.set(h, [a,b]); return h; },
        list_create_3: (a,b,c) => { const h = nextH++; listMap.set(h, [a,b,c]); return h; },
        list_create_4: (a,b,c,d) => { const h = nextH++; listMap.set(h, [a,b,c,d]); return h; },
        list_create_5: (a,b,c,d,e) => { const h = nextH++; listMap.set(h, [a,b,c,d,e]); return h; },
        list_get: (h, i) => { const l = listMap.get(h); return l ? l[Number(i)] : 0n; },
        list_append: (h, v) => { let l = listMap.get(h); if (!l) { l = []; listMap.set(h, l); } l.push(v); return h; },
        list_len: (h) => { const l = listMap.get(h); return BigInt(l ? l.length : 0); },
    },
    env: {
        now: () => BigInt(Date.now()),
    }
};

WebAssembly.instantiate(wasmBytes, importObject).then(res => {
    memoryInstance = res.instance.exports.memory;
    if (res.instance.exports.main) {
        res.instance.exports.main();
    }
    process.stdout.write(captured.trim().replace(/\r\n/g, '\n'));
}).catch(err => {
    console.error(err);
    process.exit(1);
});
"#.replace("__WASM_PATH__", &wasm_path.to_string_lossy().replace('\\', "/"));

    let output = Command::new("node")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("Node execution failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Node error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace("\r\n", "\n"))
}

pub fn compile_and_run_cranelift(
    source: &str,
    mode: &str,
    tag: &str,
) -> Result<(String, String, i32), String> {
    let sandbox = create_sandbox(tag);
    let dtr_file = sandbox.join(format!("{}.dtr", tag));
    let exe_file = sandbox.join(format!("{}.exe", tag));
    fs::write(&dtr_file, source).map_err(|e| e.to_string())?;

    let compiler = ForgenCompiler::new(mode);
    let res = compiler.compile_file(&dtr_file, Some(&exe_file));
    if !res.success {
        let _ = fs::remove_dir_all(&sandbox);
        return Err(format!("Cranelift compilation failed: {:?}", res.error));
    }

    let run_res = compiler.cranelift.run_executable(&exe_file, &[]);
    let _ = fs::remove_dir_all(&sandbox);

    match run_res {
        Ok((out, err, code, _)) => Ok((
            out.trim().replace("\r\n", "\n"),
            err.trim().replace("\r\n", "\n"),
            code,
        )),
        Err(e) => Err(e),
    }
}

pub fn compile_and_run_llvm(
    source: &str,
    mode: &str,
    tag: &str,
) -> Result<(String, String, i32), String> {
    let sandbox = create_sandbox(tag);
    let dtr_file = sandbox.join(format!("{}.dtr", tag));
    let exe_file = sandbox.join(format!("{}.exe", tag));
    fs::write(&dtr_file, source).map_err(|e| e.to_string())?;

    let compiler = ForgenCompiler::new(mode).with_llvm(true);
    let res = compiler.compile_file(&dtr_file, Some(&exe_file));
    if !res.success {
        let _ = fs::remove_dir_all(&sandbox);
        return Err(format!("LLVM compilation failed: {:?}", res.error));
    }

    let mut cmd = Command::new(&exe_file);
    cmd.env_remove("LD_PRELOAD");
    // Same pattern as run_executable: set detect_leaks=0 to suppress LeakSanitizer
    // on compiled child binaries without removing ASAN_OPTIONS entirely.
    if std::env::var("ASAN_OPTIONS").is_ok() {
        cmd.env("ASAN_OPTIONS", "detect_leaks=0");
    } else {
        cmd.env_remove("ASAN_OPTIONS");
    }
    let run = cmd.output();
    let _ = fs::remove_dir_all(&sandbox);

    match run {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout)
                .trim()
                .replace("\r\n", "\n");
            let stderr = String::from_utf8_lossy(&out.stderr)
                .trim()
                .replace("\r\n", "\n");
            let code = match out.status.code() {
                Some(c) => c,
                None => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        -out.status.signal().unwrap_or(1)
                    }
                    #[cfg(not(unix))]
                    {
                        -1
                    }
                }
            };
            let mut err_msg = stderr;
            if code != 0 && err_msg.is_empty() {
                err_msg = format!(
                    "Process terminated with status: {} (stdout: '{}')",
                    out.status, stdout
                );
            }
            Ok((stdout, err_msg, code))
        }
        Err(e) => Err(format!("Failed to execute LLVM binary: {}", e)),
    }
}

pub fn compile_and_run_wasm(source: &str, tag: &str) -> Result<String, String> {
    let sandbox = create_sandbox(tag);
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler
        .compile_source_to_dmir(source, &format!("{}.dtr", tag))
        .map_err(|e| format!("DMIR lowering failed: {:?}", e))?;
    let wasm_file = sandbox.join(format!("{}.wasm", tag));
    WasmEmitter::emit_wasm_binary(&dmir, &wasm_file)
        .map_err(|e| format!("WASM emission failed: {:?}", e))?;

    let res = run_wasm_node(&wasm_file);
    let _ = fs::remove_dir_all(&sandbox);
    res
}
