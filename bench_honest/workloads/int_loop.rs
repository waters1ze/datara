// Mirror of int_loop.dtr: identical algorithm, identical runtime-derived trip
// count. Prints the checksum on line 1 and the kernel time in ms on line 2.
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[inline(never)]
fn compute(n: i64) -> i64 {
    let mut sum: i64 = 0;
    let mut i: i64 = 0;
    while i < n {
        sum = sum.wrapping_add(i);
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
