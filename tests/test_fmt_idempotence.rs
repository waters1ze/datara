use forgen::fmt::rules::{FormatOptions, format_source};

fn assert_idempotence(source: &str, opts: &FormatOptions) {
    let (first_pass, _) = format_source(source, opts);
    let (second_pass, second_diffs) = format_source(&first_pass, opts);
    assert_eq!(
        first_pass, second_pass,
        "Formatter must be idempotent! Discrepancy detected.\n--- First pass ---\n{}\n--- Second pass ---\n{}\nDiffs on second pass: {:?}",
        first_pass, second_pass, second_diffs
    );
}

#[test]
fn test_fmt_idempotence_basic_functions_and_classes() {
    let opts = FormatOptions::default();

    let sample1 = r#"
fn calculate(a: Int, b: Int) -> Int {
let x = a + b
if x > 10 {
out x
}
return x
}
"#;
    assert_idempotence(sample1, &opts);

    let sample2 = r#"
class Point {
x: Int,
y: Int,
}

behavior Point {
fn dist(self: Point) -> Int {
return self.x * self.x + self.y * self.y
}
}
"#;
    assert_idempotence(sample2, &opts);
}

#[test]
fn test_fmt_idempotence_loops_matches_and_operators() {
    let opts = FormatOptions::default();

    let sample = r#"
fn process(items: List<Int>) -> Int {
mut sum = 0
for x in items {
if x > 0 {
sum = sum + x * 2
} else {
sum = sum - 1
}
}
return sum
}
"#;
    assert_idempotence(sample, &opts);

    let sample_match = r#"
enum Status {
Active,
Pending,
Closed,
}

fn check(s: Status) -> Int {
match s {
Status.Active => 1,
Status.Pending => 2,
Status.Closed => 3,
}
}
"#;
    assert_idempotence(sample_match, &opts);
}

#[test]
fn test_fmt_idempotence_comments_and_strings() {
    let opts = FormatOptions::default();

    let sample = r#"
// Top level comment
fn test_str() {
let s = "Hello, world! // not a comment"
let multi = "line1\nline2"
/* block comment
   spanning multiple lines */
out s
}
"#;
    assert_idempotence(sample, &opts);
}

#[test]
fn test_fmt_idempotence_property_randomized_variations() {
    let opts = FormatOptions::default();

    let ops = ["+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">="];
    let vars = ["a", "b", "c", "x", "y", "total", "count"];
    let nums = ["0", "1", "42", "100", "999"];

    // Deterministic pseudo-random generation for repeatable property testing
    let mut state: u64 = 0xDEADBEEFCAFEBABE;
    let mut next_rand = || -> u64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for iter in 0..50 {
        let mut lines = Vec::new();
        lines.push(format!("fn test_generated_{}() {{", iter));

        let num_statements = (next_rand() % 8) + 3;
        let mut indent_depth = 1;

        for s_idx in 0..num_statements {
            let roll = next_rand() % 4;
            match roll {
                0 => {
                    let v = vars[(next_rand() as usize) % vars.len()];
                    let n = nums[(next_rand() as usize) % nums.len()];
                    let op = ops[(next_rand() as usize) % ops.len()];
                    let other = vars[(next_rand() as usize) % vars.len()];
                    let pad = " ".repeat(indent_depth * 4);
                    lines.push(format!("{}let {} = {} {} {}", pad, v, other, op, n));
                }
                1 => {
                    let cond_var = vars[(next_rand() as usize) % vars.len()];
                    let pad = " ".repeat(indent_depth * 4);
                    lines.push(format!("{}if {} > 0 {{", pad, cond_var));
                    indent_depth += 1;
                    lines.push(format!("{}out {}", " ".repeat(indent_depth * 4), cond_var));
                    indent_depth -= 1;
                    lines.push(format!("{}}}", pad));
                }
                2 => {
                    let pad = " ".repeat(indent_depth * 4);
                    lines.push(format!("{}// generated comment #{}", pad, s_idx));
                }
                _ => {
                    lines.push(String::new()); // blank line
                }
            }
        }

        lines.push("}".to_string());
        let generated_source = lines.join("\n");
        assert_idempotence(&generated_source, &opts);
    }
}
