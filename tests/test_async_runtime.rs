use forgen::ForgenCompiler;

#[test]
fn test_async_proactor_runtime_compilation() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
use stdlib.async.future
use stdlib.async.task
use stdlib.async.event_loop

fn main() -> Int {
    let f1 = Future.ready("result_1")
    let f2 = Future.ready("result_2")
    let f_joined = f1.join(f2)

    mut t = Task.spawn(1, "fetch_records")
    t = t.complete(f_joined.unwrap())

    mut ev_loop = EventLoop.new()
    ev_loop = ev_loop.schedule(3)
    let total_ticks = ev_loop.run_until_complete()

    if t.is_done() && total_ticks == 3 {
        return 0
    }
    return 1
}
"#;
    let res = compiler.check_source(src, "test_async.dtr");
    assert!(
        res.success,
        "Async runtime check failed: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_reactive_native_ui_compilation() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
use stdlib.ui.native
use stdlib.ui.reactive

fn main() -> Str {
    let win = NativeWindow.create("Datara Native Dashboard", 800, 600)
    mut canvas = win.canvas
    canvas = canvas.draw_rect(10, 10, 100, 50, "blue")
    canvas = canvas.draw_text(20, 20, "Hello Native UI", "white")

    let v1 = VNode.new("div", "header", "Initial")
    let v2 = v1.set_text("Updated")
    let patch = v1.diff(v2)

    return canvas.render() + "|" + patch
}
"#;
    let res = compiler.check_source(src, "test_ui.dtr");
    assert!(res.success, "Native UI check failed: {:?}", res.diagnostics);
}

#[test]
fn test_async_runtime_execution() {
    let compiler = ForgenCompiler::new("jit");
    let src = r#"
use stdlib.async.future
use stdlib.async.task
use stdlib.async.event_loop

fn main() -> Int {
    let f = Future.ready("data")
    let mapped = f.map("_processed")
    mut ev_loop = EventLoop.new()
    ev_loop = ev_loop.schedule(2)
    let ticks = ev_loop.run_until_complete()
    if mapped.is_ready() && ticks == 2 {
        return 0
    }
    return 1
}
"#;
    let out_dir = std::env::temp_dir().join("datara_test_async_bin");
    let _ = std::fs::create_dir_all(&out_dir);
    let exe_path = out_dir.join("test_async.exe");

    let res = compiler.compile_source(src, "test_async_exec.dtr", Some(&exe_path));
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    if let Some(ref path) = res.exe_path {
        let output = std::process::Command::new(path)
            .output()
            .expect("Failed to execute compiled binary");
        assert_eq!(output.status.code(), Some(0));
    }
}
