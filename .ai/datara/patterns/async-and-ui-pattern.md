# Datara Async Proactor & Reactive UI Pattern

Datara features a high-performance proactor asynchronous runtime and zero-JS reactive UI primitives.

---

## 1. Async Proactor Runtime Architecture

The asynchronous execution model is built upon three core primitives:
- `Future`: State-machine container (`ready`, `pending`, `failed`)
- `Task`: Schedulable unit of background work
- `EventLoop`: Tick-driven proactor executor

```datara
use stdlib.async.future
use stdlib.async.task
use stdlib.async.event_loop

fn main() {
    // 1. Create completed or pending futures
    let f1 = Future.ready("result_alpha")
    let f2 = Future.ready("result_beta")
    let joined = f1.join(f2)

    // 2. Spawn and complete background tasks
    mut worker = Task.spawn(100, "async_worker")
    worker = worker.complete(joined.unwrap())

    // 3. Drive event loop
    mut loop_engine = EventLoop.new()
    loop_engine = loop_engine.schedule(3)
    let total_ticks = loop_engine.run_until_complete()

    out fmt"Task done: {worker.is_done()}, Ticks: {total_ticks}"
}
```

---

## 2. Native & Reactive UI Architecture

Datara includes lightweight, zero-JS UI abstractions suitable for native windowing or embedded displays:

```datara
use stdlib.ui.native
use stdlib.ui.reactive

fn main() {
    // Native Window Canvas drawing
    let window = NativeWindow.create("Control Panel", 800, 600)
    mut canvas = window.canvas
    canvas = canvas.draw_rect(10, 10, 200, 100, "blue")
    canvas = canvas.draw_text(20, 20, "System Status: Online", "white")

    // Virtual Node Reactive diffing
    let initial_node = VNode.new("div", "status_card", "Running")
    let updated_node = initial_node.set_text("Completed")
    let patch = initial_node.diff(updated_node)

    out fmt"Generated Patch: {patch}"
}
```
