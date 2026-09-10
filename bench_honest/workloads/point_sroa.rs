// Mirror of point_sroa.dtr. The struct is expected to be scalarized away.
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct Point {
    x: i64,
    y: i64,
}

#[inline(never)]
fn compute(n: i64) -> i64 {
    let mut total: i64 = 0;
    let mut i: i64 = 0;
    while i < n {
        let p = Point { x: i, y: 1 };
        total = total.wrapping_add(p.x.wrapping_add(p.y));
        i = i + 1;
    }
    total
}

fn main() {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let n: i64 = 200_000_000 + now_ms / 10_000_000_000_000;

    let a = Instant::now();
    let r = compute(n);
    let b = Instant::now();

    println!("{}", r);
    println!("{:.3}", (b - a).as_secs_f64() * 1000.0);
}
