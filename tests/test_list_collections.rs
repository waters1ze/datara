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

#[test]
fn test_list_higher_order_map() {
    let out = run_datara(
        r#"
fn main() {
    let numbers = [1, 2, 3, 4]
    let doubled = numbers.map(x => x * 2)
    for n in doubled {
        out n
    }
}
"#,
        "test_list_map.dtr",
    );

    assert_eq!(out, "2\n4\n6\n8");
}

#[test]
fn test_list_higher_order_filter() {
    let out = run_datara(
        r#"
fn main() {
    let numbers = [10, 15, 20, 25, 30]
    let evens = numbers.filter(x => x % 20 == 0)
    for n in evens {
        out n
    }
}
"#,
        "test_list_filter.dtr",
    );

    assert_eq!(out, "20");
}

#[test]
fn test_list_higher_order_reduce() {
    let out = run_datara(
        r#"
fn main() {
    let numbers = [1, 2, 3, 4, 5]
    let sum = numbers.reduce(0, (acc, x) => acc + x)
    out sum
}
"#,
        "test_list_reduce.dtr",
    );

    assert_eq!(out, "15");
}

#[test]
fn test_list_higher_order_find() {
    let out = run_datara(
        r#"
fn main() {
    let numbers = [10, 25, 30, 45]
    let found = numbers.find(x => x > 20)
    out found
    let not_found = numbers.find(x => x > 100)
    out not_found
}
"#,
        "test_list_find.dtr",
    );

    assert_eq!(out, "25\n-1");
}

#[test]
fn test_list_higher_order_any_and_all() {
    let out = run_datara(
        r#"
fn main() {
    let numbers = [2, 4, 6, 8]
    let all_even = numbers.all(x => x % 2 == 0)
    out all_even
    let any_gt_five = numbers.any(x => x > 5)
    out any_gt_five
    let any_odd = numbers.any(x => x % 2 != 0)
    out any_odd
}
"#,
        "test_list_any_all.dtr",
    );

    assert_eq!(out, "true\ntrue\nfalse");
}

#[test]
fn test_list_higher_order_chaining() {
    let out = run_datara(
        r#"
fn main() {
    let numbers = [1, 2, 3, 4, 5, 6]
    let res = numbers.filter(x => x % 2 == 0).map(x => x * 10).reduce(0, (acc, x) => acc + x)
    out res
}
"#,
        "test_list_chaining.dtr",
    );

    assert_eq!(out, "120");
}

#[test]
fn test_list_closure_capturing_enclosing_var() {
    let out = run_datara(
        r#"
fn main() {
    let factor = 7
    let numbers = [1, 2, 3]
    let scaled = numbers.map(x => x * factor)
    for n in scaled {
        out n
    }
}
"#,
        "test_list_closure_capture.dtr",
    );

    assert_eq!(out, "7\n14\n21");
}

#[test]
fn test_standalone_higher_order_functions() {
    let out = run_datara(
        r#"
fn main() {
    let xs = [5, 10, 15]
    let mapped = map(xs, x => x + 1)
    let total = reduce(mapped, 0, (acc, x) => acc + x)
    out total
}
"#,
        "test_list_standalone_ho.dtr",
    );

    assert_eq!(out, "33");
}
