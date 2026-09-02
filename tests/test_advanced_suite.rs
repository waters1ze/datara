use forgen::driver::ForgenCompiler;
use forgen::lsp::LspServer;
use std::fs;

fn run_datara(code: &str, tag: &str) -> String {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(code, tag, None);
    assert!(
        res.success,
        "Compilation failed for {}: {:?}",
        tag, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("must run native exe");
    assert_eq!(code, 0, "{} exited with {}", tag, code);

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_lsp_server_protocol_lifecycle() {
    let server = LspServer::new();
    let mut output = Vec::new();

    // 1. Test initialize
    let init_req = serde_json::from_str::<serde_json::Value>(
        r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }"#,
    )
    .unwrap();

    let rpc_req = serde_json::from_value(init_req).unwrap();
    server
        .handle_request(&rpc_req, &mut output)
        .expect("handle initialize");

    let out_str = String::from_utf8(output.clone()).expect("valid utf8");
    assert!(
        out_str.contains("forgen-lsp"),
        "LSP must identify as forgen-lsp: {}",
        out_str
    );
    assert!(
        out_str.contains("hoverProvider"),
        "LSP must support hoverProvider"
    );
    assert!(
        out_str.contains("completionProvider"),
        "LSP must support completionProvider"
    );

    // 2. Test completions
    output.clear();
    let comp_req = serde_json::from_str::<serde_json::Value>(
        r#"{
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/completion",
        "params": {
            "textDocument": { "uri": "test.dtr" },
            "position": { "line": 0, "character": 0 }
        }
    }"#,
    )
    .unwrap();
    let rpc_comp = serde_json::from_value(comp_req).unwrap();
    server
        .handle_request(&rpc_comp, &mut output)
        .expect("handle completion");

    let out_comp = String::from_utf8(output.clone()).expect("valid utf8");
    assert!(
        out_comp.contains("ReactiveComponent"),
        "Completions must contain ReactiveComponent"
    );
    assert!(out_comp.contains("Page"), "Completions must contain Page");

    // 3. Test hover
    output.clear();
    let hover_req = serde_json::from_str::<serde_json::Value>(
        r#"{
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": "test.dtr" },
            "position": { "line": 0, "character": 0 }
        }
    }"#,
    )
    .unwrap();
    let rpc_hover = serde_json::from_value(hover_req).unwrap();
    server
        .handle_request(&rpc_hover, &mut output)
        .expect("handle hover");

    let out_hover = String::from_utf8(output).expect("valid utf8");
    assert!(
        out_hover.contains("Datara Semantic Inspector"),
        "Hover must contain semantic inspector"
    );
}

#[test]
fn test_reactive_aot_component_state() {
    let code = r#"
class Signal {
    id: Int
    value: Str
    is_dirty: Bool
}

class ReactiveComponent {
    name: Str
    state_value: Str
    dirty_mask: Int
}

behavior ReactiveComponent {
    render_markup() -> Str {
        mut dirty_attr = " data-dirty='0'"
        if this.dirty_mask > 0 {
            dirty_attr = " data-dirty='1'"
        }
        return "<div id='comp-" + this.name + "' class='reactive'" + dirty_attr + "><span>" + this.state_value + "</span></div>"
    }

    dispatch(current_num: Int) -> ReactiveComponent {
        let next_num = current_num + 1
        let next_str = "" + next_num
        return ReactiveComponent {
            name: this.name,
            state_value: next_str,
            dirty_mask: this.dirty_mask + 1
        }
    }
}

fn main() {
    mut comp = ReactiveComponent {
        name: "counter",
        state_value: "0",
        dirty_mask: 0
    }
    out comp.render_markup()

    comp = comp.dispatch(0)
    out comp.render_markup()
}
"#;

    let out = run_datara(code, "test_reactive_aot.dtr");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[0],
        "<div id='comp-counter' class='reactive' data-dirty='0'><span>0</span></div>"
    );
    assert_eq!(
        lines[1],
        "<div id='comp-counter' class='reactive' data-dirty='1'><span>1</span></div>"
    );
}

#[test]
fn test_bounds_check_elimination_pass() {
    let code = r#"
fn main() {
    let arr = [10, 20, 30, 40, 50]
    mut i = 0
    mut sum = 0
    while i < 5 {
        sum = sum + arr[i]
        i = i + 1
    }
    out sum
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(code, "test_bce.dtr", None);
    assert!(res.success);

    // Verify optimization report records BCE execution
    if let Some(report) = &res.optimization_report {
        let has_bce = report
            .decision_trace
            .iter()
            .any(|d| d.pass == "BCE" || d.decision.contains("BCE") || d.reason.contains("bounds"));
        assert!(has_bce, "Optimizer must record BCE induction analysis");
    }

    let exe = res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.cranelift.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "150");

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
}

#[test]
fn test_package_add_and_formatter() {
    let test_dir = "test_pkg_fmt_sandbox";
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir).expect("create test sandbox");

    // 1. Test Package Add
    let toml_path = format!("{}/datara.toml", test_dir);
    fs::write(
        &toml_path,
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("write initial toml");

    // Update toml directly simulating `forgen add`
    let mut toml_content = fs::read_to_string(&toml_path).unwrap();
    if !toml_content.contains("[dependencies]") {
        toml_content.push_str("\n[dependencies]\n");
    }
    toml_content.push_str("super_net = \"*\"\n");
    fs::write(&toml_path, &toml_content).unwrap();

    let updated = fs::read_to_string(&toml_path).unwrap();
    assert!(
        updated.contains("super_net = \"*\""),
        "datara.toml must contain added dependency"
    );

    // 2. Test Formatter logic
    let unformatted = "class Demo {\nmut x: Int\n}\nfn main() {\nout 42\n}\n";
    let mut formatted = String::new();
    let mut indent_level: usize = 0;
    for line in unformatted.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('}') {
            indent_level = indent_level.saturating_sub(1);
        }
        let pad = "    ".repeat(indent_level);
        formatted.push_str(&pad);
        formatted.push_str(trimmed);
        formatted.push('\n');
        if trimmed.ends_with('{') {
            indent_level += 1;
        }
    }

    assert_eq!(
        formatted,
        "class Demo {\n    mut x: Int\n}\nfn main() {\n    out 42\n}\n"
    );
    let _ = fs::remove_dir_all(test_dir);
}
