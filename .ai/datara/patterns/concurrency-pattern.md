# Datara Fearless Concurrency & Parallelism Pattern

Datara guarantees freedom from data races at compile time via **Proof-Carrying Concurrency** (`Error[E0943]`).

---

## 1. Multi-Core Range Loop (`parallel for`)

```datara
fn process_batch(items: List<Int>) {
    parallel for i in 0..100 {
        // Thread-local state is completely safe
        mut local_acc = 0
        local_acc = local_acc + i
    }
}
```

---

## 2. Data Race Prevention (`E0943`)

Mutating an outer variable from within a `parallel` block causes a fatal compile error:

```datara
mut global_sum = 0
parallel for i in 0..100 {
    global_sum = global_sum + i // Fatal Error[E0943]: DataRaceViolation
}
```

### Safe Reduction Pattern
Isolate local accumulators or use atomic reduction functions:
```datara
// Safe: each worker works on thread-local memory
parallel for i in 0..threads {
    mut local_total = compute_chunk(i)
}
```

---

## 3. Fork-Join Parallelism (`parallel { }`)

Execute independent tasks concurrently across available CPU cores:

```datara
parallel {
    load_customer_data()
    fetch_inventory_status()
    query_exchange_rates()
}
```
