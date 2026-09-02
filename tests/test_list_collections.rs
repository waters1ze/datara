//! List collection semantics: iteration, mutation, and length.
//!
//! `for x in <list>` must lower into a real counted loop over the runtime
//! list protocol (not the historical "evaluate and run the body once"
//! placeholder). List methods dispatch on the object's runtime shape:
//! `length()`/`count()`, `get(i)`, `set(i, v)`, `push(v)`/`append(v)`.

use forgen::driver::ForgenCompiler;

fn run_datara(source: &str, name: &str) -> String {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, name, None);
    assert!(
        res.success,
        "compilation failed for {}: {:?}",
        name, res.error
    );

    let exe = res.exe_path.clone().expect("must produce a native .exe");
    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&exe, &[])
        .expect("must run native exe");
    assert_eq!(code, 0, "{} exited with {}", name, code);

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(exe.with_extension("obj"));
    stdout.trim().replace("\r\n", "\n")
}

#[test]
fn test_for_in_list_iterates_all_elements() {
    let out = run_datara(
        r#"
fn main() {
    let numbers = [10, 20, 30, 40]
    for n in numbers {
        out n
    }
}
"#,
        "test_list_forin.dtr",
    );

    assert_eq!(out, "10\n20\n30\n40");
}

#[test]
fn test_for_in_list_aggregation() {
    let out = run_datara(
        r#"
fn main() {
    mut xs = [1, 2, 3, 4, 5]
    mut sum = 0
    for x in xs {
        sum = sum + x
    }
    out sum
}
"#,
        "test_list_sum.dtr",
    );

    assert_eq!(out, "15");
}

#[test]
fn test_list_length_get_set() {
    let out = run_datara(
        r#"
fn main() {
    mut xs = [5, 6, 7]
    out xs.length()
    out xs.get(0)
    out xs.get(2)
    xs = xs.set(1, 60)
    out xs.get(1)
}
"#,
        "test_list_len_get_set.dtr",
    );

    assert_eq!(out, "3\n5\n7\n60");
}

#[test]
fn test_list_push_grows() {
    let out = run_datara(
        r#"
fn main() {
    mut xs = [1, 2]
    xs = xs.push(3)
    xs = xs.push(99)
    out xs.length()
    out xs.get(3)
    mut total = 0
    for x in xs {
        total = total + x
    }
    out total
}
"#,
        "test_list_push.dtr",
    );

    assert_eq!(out, "4\n99\n105");
}

#[test]
fn test_list_loop_closure_still_optimizes() {
    // A for-in loop whose body is a pure sum must produce exactly 105 —
    // the loop-closure pass must not corrupt element iteration.
    let out = run_datara(
        r#"
fn main() {
    mut xs = [10, 20, 30, 40, 5]
    mut sum = 0
    for x in xs {
        sum = sum + x
    }
    out sum
}
"#,
        "test_list_closure.dtr",
    );

    assert_eq!(out, "105");
}

#[test]
fn test_empty_list_runs_zero_iterations() {
    // A list of length 0 must run the body zero times (it historically
    // ran exactly once).
    let out = run_datara(
        r#"
fn main() {
    mut xs = []
    mut count = 0
    for x in xs {
        count = count + 1
    }
    out count
}
"#,
        "test_list_empty.dtr",
    );

    assert_eq!(out, "0");
}
