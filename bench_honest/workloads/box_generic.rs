// Mirror of box_generic.dtr: the generic container is monomorphized to i64.
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct BoxI<T> {
    val: T,
}

#[inline(never)]
fn compute(n: i64) -> i64 {
    let mut total: i64 = 0;
    let mut i: i64 = 0;
    while i < n {
        let b = BoxI::<i64> { val: i };
        total = total.wrapping_add(b.val);
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
