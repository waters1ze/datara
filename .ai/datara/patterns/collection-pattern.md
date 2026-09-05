# Datara Collections & Pipeline Pattern

Datara standard library provides high-performance collections optimized for zero-copy views and Evidence Gate stream vectorization.

---

## 1. Dynamic Lists (`List<T>`)

```datara
fn main() {
    val numbers = [10, 20, 30, 40, 50]
    out numbers[2] // 30
}
```

### Range Slicing & Iteration
```datara
val items = [1, 2, 3, 4, 5]
for item in items {
    out item
}
```

---

## 2. Pipeline Transformations (`|>` and `then`)

Pipelines chain operations with zero intermediate heap allocations via Stream Fusion:

```datara
fn double(x: Int) -> Int => x * 2
fn add_one(x: Int) -> Int => x + 1

fn main() {
    // Pipe-forward operator
    let res1 = 10 |> double() |> add_one()
    out fmt"Pipe result: {res1}" // 21

    // Natural language 'then' pipeline (compiles to identical IR)
    let res2 = 10 then double() then add_one()
    out fmt"Then result: {res2}" // 21
}
```

---

## 3. Associated Maps (`Map<K, V>`)

```datara
use stdlib.collections.map.MapWrapper

fn main() {
    mut store = MapWrapper<Str, Int> { capacity: 16 }
    store.insert("admin_level", 99)
    let lvl = store.get("admin_level")
    out fmt"Level: {lvl}"
}
```
