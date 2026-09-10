// Mirror of float_loop.dtr. Sequential float adds in the same order, so the
// checksum is bit-identical when neither side reassociates.
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[inline(never)]
fn compute(n: i64) -> f64 {
    let mut sum: f64 = 0.0;
    let mut i: i64 = 0;
    while i < n {
        sum = sum + (i as f64) * 2.0;
        i = i + 1;
    }
    sum
}

fn main() {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let n: i64 = 500_000_000 + now_ms / 10_000_000_000_000;

    let a = Instant::now();
    let r = compute(n);
    let b = Instant::now();

    println!("{}", r);
    println!("{:.3}", (b - a).as_secs_f64() * 1000.0);
}
