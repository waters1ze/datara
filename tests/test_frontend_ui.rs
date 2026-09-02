use forgen::driver::ForgenCompiler;
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
fn test_ui_element_and_component_composition() {
    let code = r#"
class UIElement {
    tag: Str
    classes: Str
    content: Str
}

behavior UIElement {
    render() -> Str {
        return "<" + this.tag + " class='" + this.classes + "'>" + this.content + "</" + this.tag + ">"
    }
}

class MetricCard {
    label: Str
    value: Str
}

behavior MetricCard {
    render() -> Str {
        return "<div class='metric'><span class='lbl'>" + this.label + "</span><b class='val'>" + this.value + "</b></div>"
    }
}

fn main() {
    let el = UIElement {
        tag: "button",
        classes: "btn btn-primary",
        content: "Submit"
    }
    let m = MetricCard {
        label: "FPS",
        value: "120"
    }
    out el.render()
    out m.render()
}
"#;

    let out = run_datara(code, "test_ui_composition.dtr");
    assert!(
        out.contains("<button class='btn btn-primary'>Submit</button>"),
        "Actual: {}",
        out
    );
    assert!(
        out.contains(
            "<div class='metric'><span class='lbl'>FPS</span><b class='val'>120</b></div>"
        ),
        "Actual: {}",
        out
    );
}

#[test]
fn test_ui_page_file_generation() {
    let target_html = "test_gen_page.html";
    let _ = fs::remove_file(target_html);

    let code = format!(
        r#"
class Page {{
    title: Str
    content: Str
}}

behavior Page {{
    save(path: Str) -> Bool {{
        let doc = "<!DOCTYPE html><html><head><title>" + this.title + "</title></head><body>" + this.content + "</body></html>"
        let res = file_write(path, doc)
        return res == 1
    }}
}}

fn main() {{
    let p = Page {{
        title: "Test Dashboard",
        content: "<h1>Zero-JS Pure Datara</h1>"
    }}
    let ok = p.save("{}")
    assert(ok, "Failed to write html file")
    out "PAGE_SAVED_OK"
}}
"#,
        target_html
    );

    let out = run_datara(&code, "test_ui_page_save.dtr");
    assert_eq!(out, "PAGE_SAVED_OK");

    let saved = fs::read_to_string(target_html).expect("must read generated html");
    assert!(saved.contains("<!DOCTYPE html><html><head><title>Test Dashboard</title></head><body><h1>Zero-JS Pure Datara</h1></body></html>"));
    let _ = fs::remove_file(target_html);
}

#[test]
fn test_native_gui_linking() {
    // Tests that linking against user32.lib compiles into a valid native executable without missing symbols
    let code = r#"
extern "C" fn MessageBoxA(hwnd: Int, text: Str, caption: Str, utype: Int) -> Int

fn main() {
    // Guarded so it does not block CI with a GUI modal, but forces the linker
    // to resolve MessageBoxA from user32.lib
    if 1 == 2 {
        let res = MessageBoxA(0, "Headless test", "Title", 0)
    }
    out "NATIVE_GUI_LINKED_OK"
}
"#;

    let out = run_datara(code, "test_native_gui_link.dtr");
    assert_eq!(out, "NATIVE_GUI_LINKED_OK");
}
