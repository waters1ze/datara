use forgen::repl::{ReplSession, count_brace_delta};

#[test]
fn test_repl_count_brace_delta() {
    assert_eq!(count_brace_delta("fn main() {"), 1);
    assert_eq!(count_brace_delta("}"), -1);
    assert_eq!(count_brace_delta("println(\"}\")"), 0);
    assert_eq!(count_brace_delta("println(\"{\")"), 0);
    assert_eq!(count_brace_delta("// { comment"), 0);
    assert_eq!(count_brace_delta("decide { // { inside comment"), 1);
}

#[test]
fn test_repl_multiline_user_reported_case() {
    let mut session = ReplSession::new();

    // User's exact input sequence:
    // 1. fn grade(score: Int) -> Str {
    assert_eq!(session.feed_line("fn grade(score: Int) -> Str {"), None);
    assert_eq!(session.brace_depth, 1);

    // 2.     decide {
    assert_eq!(session.feed_line("    decide {"), None);
    assert_eq!(session.brace_depth, 2);

    // 3.         score >= 90 => "A"
    assert_eq!(session.feed_line("        score >= 90 => \"A\""), None);
    assert_eq!(session.brace_depth, 2);

    // 4.         score >= 75 => "B"
    assert_eq!(session.feed_line("        score >= 75 => \"B\""), None);

    // 5.         score >= 60 => "C"
    assert_eq!(session.feed_line("        score >= 60 => \"C\""), None);

    // 6.         else => "F"
    assert_eq!(session.feed_line("        else => \"F\""), None);

    // 7.     }
    assert_eq!(session.feed_line("    }"), None);
    assert_eq!(session.brace_depth, 1);

    // 8. }
    let res = session.feed_line("}");
    assert_eq!(session.brace_depth, 0);
    assert!(res.is_some());
    let msg = res.unwrap();
    assert!(
        msg.contains("registered declaration: grade"),
        "Got: {}",
        msg
    );

    // Now test calling grade(85) directly
    let res85 = session.feed_line("grade(85)");
    assert!(res85.is_some());
    assert_eq!(res85.unwrap(), "=> B");

    // Test calling grade(50) directly
    let res50 = session.feed_line("grade(50)");
    assert!(res50.is_some());
    assert_eq!(res50.unwrap(), "=> F");

    // Now test pasting `fn main() { out fmt"Score 85 Grade: {grade(85)}" }`
    assert_eq!(session.feed_line("fn main() {"), None);
    assert_eq!(
        session.feed_line("    println(fmt\"Score 85 Grade: {grade(85)}\")"),
        None
    );
    assert_eq!(
        session.feed_line("    println(fmt\"Score 50 Grade: {grade(50)}\")"),
        None
    );
    let run_res = session.feed_line("}");
    assert!(run_res.is_some());
    let output = run_res.unwrap();
    assert!(output.contains("Score 85 Grade: B"), "Got: {}", output);
    assert!(output.contains("Score 50 Grade: F"), "Got: {}", output);
}
