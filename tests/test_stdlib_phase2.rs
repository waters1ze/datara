//! Phase 2 Stdlib & DX Integration Tests
//!
//! Verifies:
//! 1. `Set<T>` distinct collection with add/contains/remove.
//! 2. `Deque<T>` double-ended queue with push/pop front and back.
//! 3. `PriorityQueue<T>` binary heap with push/pop/peek.
//! 4. `Iterator<T>` lazy chains with take, skip, chunk, collect, and zip.
//! 5. `StringBuilder` with append, append_int, append_line, to_str.
//! 6. Format stream interpolation `fmt"..."` across primitives in a table × string × map e2e test.

use forgen::driver::ForgenCompiler;
use std::fs;
use std::process::Command;

fn run_datara(source: &str, name: &str) -> String {
    let compiler = ForgenCompiler::new("jit");
    let out_dir = std::env::temp_dir().join("datara_test_stdlib_p2");
    let _ = fs::create_dir_all(&out_dir);
    let exe = out_dir.join(format!("{}.exe", name));

    let res = compiler.compile_source(source, &format!("{}.dtr", name), Some(&exe));
    assert!(
        res.success,
        "compilation failed for {}: error={:?}, diags={:?}",
        name, res.error, res.diagnostics
    );

    let output = Command::new(&exe).output().expect("must run native exe");
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{} exited with error: {}\nSTDOUT was: {}",
        name,
        String::from_utf8_lossy(&output.stderr),
        stdout_str
    );

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace("\r\n", "\n")
}

#[test]
fn test_stdlib_set_distinct_collection() {
    let out = run_datara(
        r#"
use stdlib.collections.set

fn main() {
    mut s = set_new()
    let a1 = s.add(10)
    let a2 = s.add(20)
    let a3 = s.add(10)
    out a1
    out a2
    out a3
    out s.len()
    out s.contains(20)
    out s.contains(99)
    let r1 = s.remove(10)
    out r1
    out s.len()
    out s.contains(10)
}
"#,
        "test_set_ops",
    );
    assert_eq!(out, "true\ntrue\nfalse\n2\ntrue\nfalse\ntrue\n1\nfalse");
}

#[test]
fn test_stdlib_deque_operations() {
    let out = run_datara(
        r#"
use stdlib.collections.deque

fn main() {
    mut q = deque_new()
    q.push_back(100)
    q.push_back(200)
    q.push_front(50)
    out q.len()
    out q.peek_front()
    out q.peek_back()
    let p1 = q.pop_front()
    let p2 = q.pop_back()
    out p1
    out p2
    out q.len()
}
"#,
        "test_deque_ops",
    );
    assert_eq!(out, "3\n50\n200\n50\n200\n1");
}

#[test]
fn test_stdlib_priority_queue_heap() {
    let out = run_datara(
        r#"
use stdlib.collections.priority_queue

fn main() {
    mut pq = priority_queue_new()
    pq.push(15)
    pq.push(40)
    pq.push(10)
    pq.push(30)
    out pq.len()
    out pq.peek()
    out pq.pop()
    out pq.pop()
    out pq.pop()
    out pq.pop()
}
"#,
        "test_pq_ops",
    );
    assert_eq!(out, "4\n40\n40\n30\n15\n10");
}

#[test]
fn test_stdlib_iterator_lazy_chains() {
    let out = run_datara(
        r#"
use stdlib.collections.iter

fn main() {
    let xs = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    let it = iter_from(xs)
    let sub = it.skip(2).take(4).collect()
    out sub.len()
    out sub.get(0)
    out sub.get(1)
    out sub.get(2)
    out sub.get(3)

    let chunks = it.chunk(3)
    out chunks.len()
    let c0 = chunks.get(0)
    out c0.len()
    out c0.get(0)

    let zipped = iter_zip([10, 20], [100, 200])
    out zipped.len()
}
"#,
        "test_iter_ops",
    );
    assert_eq!(out, "4\n3\n4\n5\n6\n4\n3\n1\n2");
}

#[test]
fn test_stdlib_string_builder() {
    let out = run_datara(
        r#"
use stdlib.text.string_builder

fn main() {
    mut sb = string_builder_new()
    sb.append("Datara")
    sb.append(" ")
    sb.append("Version: ")
    sb.append_int(1)
    sb.append_line("!")
    sb.append("Done")
    out sb.to_str()
}
"#,
        "test_sb_ops",
    );
    assert_eq!(out, "Datara Version: 1!\nDone");
}

#[test]
fn test_fmt_string_table_str_map_e2e() {
    let out = run_datara(
        r#"
class Row {
    id: Int
    name: Str
    score: Float
    active: Bool
}

fn main() {
    let r1 = Row { id: 1, name: "Alice", score: 98.5, active: true }
    let r2 = Row { id: 2, name: "Bob", score: 87.0, active: false }

    let t1 = fmt"| {r1.id} | {r1.name} | {r1.score} | {r1.active} |"
    let t2 = fmt"| {r2.id} | {r2.name} | {r2.score} | {r2.active} |"

    out t1
    out t2
}
"#,
        "test_fmt_table_e2e",
    );
    assert_eq!(out, "| 1 | Alice | 98.5 | true |\n| 2 | Bob | 87 | false |");
}
