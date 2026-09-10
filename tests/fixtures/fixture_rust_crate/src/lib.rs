pub fn add(a: i64, b: i64) -> i64 {
    a + b
}

pub fn mult(a: i64, b: i64) -> i64 {
    a * b
}

pub fn fibonacci(n: i64) -> i64 {
    if n <= 0 {
        0
    } else if n == 1 {
        1
    } else {
        let mut a = 0i64;
        let mut b = 1i64;
        for _ in 2..=n {
            let temp = a + b;
            a = b;
            b = temp;
        }
        b
    }
}

pub fn square(x: f64) -> f64 {
    x * x
}
