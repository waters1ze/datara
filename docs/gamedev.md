# Game Development in Datara: Deterministic Lockstep, SIMD & Low-Latency Architecture

Datara and its native optimizing compiler `forgen` provide first-class facilities for real-time simulations, networked multiplayer engines, and game physics. The zero-GC runtime combined with ownership-driven memory management eliminates stop-the-world pauses, while hardware-accelerated SIMD and multi-core work scheduling deliver high throughput.

---

## 1. Deterministic Lockstep Simulation

In peer-to-peer or server-authoritative multiplayer games (RTS, fighting games, physics-driven simulations), **lockstep netcode** transmits only user inputs across the network rather than full world snapshots. Every client must compute identical game states given the same inputs.

### Determinism Invariants in Datara
1. **IEEE 754 Floating-Point Consistency**: Float operations adhere strictly to 64-bit (`Float`) and 32-bit (`Float32` / SIMD lane) IEEE-754 semantics with consistent rounding modes across supported platforms.
2. **Swift-Style Checked Integer Arithmetic**: All signed and unsigned integer operations trap on overflow by default, avoiding silent desyncs caused by architecture-dependent wrap behavior.
3. **Reproducible Work Distribution**: Multi-threaded simulation passes using `parallel for` execute across partition boundaries deterministically.

---

## 2. Hardware SIMD Acceleration

Datara provides first-class 128-bit SIMD vector types and hardware-accelerated intrinsics:

| Type / Intrinsic | Description | Hardware Instruction Mapping (x86_64 / ARM64) |
|---|---|---|
| `float4(x, y, z, w)` | 4-wide 32-bit floating-point vector | `movups` / `ld1` into 128-bit SIMD register |
| `int4(a, b, c, d)` | 4-wide 32-bit integer vector | `movdqu` / `ld1` into 128-bit SIMD register |
| `dot(v1, v2)` | 4D float dot product returning `Float` | `dpps` / `fmul` + horizontal pairwise reduction |
| `min4(v1, v2)` | Lane-wise minimum of two `float4` | `minps` / `fmin` |
| `max4(v1, v2)` | Lane-wise maximum of two `float4` | `maxps` / `fmax` |

### SIMD Physics Example:
```datara
fn simd_clamp_energy(pos: Float4) -> Float {
    let bound_min = float4(-100.0, -100.0, -100.0, 0.0)
    let bound_max = float4(100.0, 100.0, 100.0, 10.0)
    let clamped_pos = min4(max4(pos, bound_min), bound_max)
    return dot(clamped_pos, clamped_pos)
}

fn main() {
    let pos = float4(15.0, -25.0, 5.0, 1.0)
    let energy = simd_clamp_energy(pos)
    out energy
}
```

---

## 3. High-Resolution Monotonic Timing

Datara exposes precise, non-decreasing sub-millisecond timers specifically designed for frame synchronization and delta-time calculations:

- `datara_rt_time_precise_ms() -> Float`: Returns the high-resolution monotonic timestamp in milliseconds (backed by `QueryPerformanceCounter` on Windows and `clock_gettime(CLOCK_MONOTONIC)` on POSIX).
- `datara_rt_time_delta_ms() -> Float`: Returns elapsed milliseconds since the previous call (thread-local and clamped $\ge 0.0$).

```datara
fn main() {
    let t0 = datara_rt_time_precise_ms()
    let dt = datara_rt_time_delta_ms()
    out "TIME_INITIALIZED"
}
```

---

## 4. Frame Arena Allocator (Zero-Allocation Per Frame)

Allocating on the general heap (`malloc`/`free`) during the inner loop of a 60/120 FPS game introduces cache thrashing and fragmentation. Datara exposes a linear thread-local **Frame Arena Allocator**:

### Runtime API
- `datara_rt_arena_alloc(size: Int) -> RawPtr`: Allocates contiguous bytes in constant time $O(1)$ by bumping an internal pointer.
- `datara_rt_arena_checkpoint() -> Int`: Returns the current arena offset mark.
- `datara_rt_arena_reset(checkpoint: Int)`: Rewinds the arena offset back to the checkpoint in $O(1)$, freeing all temporary allocations made in the frame.

### Game Loop Pattern
```datara
extern fn datara_rt_arena_checkpoint() -> Int
extern fn datara_rt_arena_alloc(bytes: Int) -> RawPtr
extern fn datara_rt_arena_reset(checkpoint: Int)

fn game_frame() {
    unsafe(justification: "Per-frame temporary scratch memory lifecycle") {
        let cp = datara_rt_arena_checkpoint()
        let mem = datara_rt_arena_alloc(1024)
        datara_rt_arena_reset(cp)
    }
}

fn main() {
    game_frame()
    out "ARENA_FRAME_OK"
}
```

---

## 5. Multi-Core Simulation with `parallel for`

Workloads such as particle simulations, flocking/boids, and entity updates can be parallelized with `parallel for`:

```datara
fn simulate_entity(id: Int, frames: Int) -> Int {
    let id_flt = str_to_float(int_to_str(id))
    let p = float4(id_flt, 0.0, 0.0, 1.0)
    let v = float4(0.1, 0.0, 0.0, 0.0)
    let d = dot(p, v)
    return str_to_int(float_to_str(d))
}

fn run_worker(worker_id: Int) {
    let entities_per_worker = 16
    let start_id = worker_id * entities_per_worker
    let end_id = start_id + entities_per_worker
    mut i = start_id
    while i < end_id {
        let _ = simulate_entity(i, 50)
        i = i + 1
    }
}

fn update_world() {
    let num_workers = 4
    parallel for w in 0..num_workers {
        run_worker(w)
    }
}

fn main() {
    update_world()
    out "WORLD_UPDATED"
}
```

---

## 6. Deterministic 60 Hz Fixed-Timestep Game Loop

A stable physics engine requires fixed $\Delta t$ updates independent of display refresh rate or hardware hitches. The accumulator pattern decouples simulation steps from frame rendering:

```datara
record GameState {
    tick: Int,
    entity_x: Float,
    velocity_x: Float,
}

fn update_physics(state: GameState, dt: Float) -> GameState {
    let new_x = state.entity_x + state.velocity_x * dt
    let new_tick = state.tick + 1
    return GameState {
        tick: new_tick,
        entity_x: new_x,
        velocity_x: state.velocity_x,
    }
}

fn run_game_loop(total_frames: Int) -> GameState {
    let fixed_dt: Float = 16.666667
    let fixed_dt_sec: Float = 0.01666667
    mut state = GameState { tick: 0, entity_x: 0.0, velocity_x: 10.0 }
    mut accumulator: Float = 0.0

    mut frame = 0
    while frame < total_frames {
        let frame_delta = 16.0
        accumulator = accumulator + frame_delta
        while accumulator >= fixed_dt {
            state = update_physics(state, fixed_dt_sec)
            accumulator = accumulator - fixed_dt
        }
        frame = frame + 1
    }
    return state
}

fn main() {
    let final_state = run_game_loop(60)
    out "GAME_LOOP_OK: ticks=" + final_state.tick
}
```

---

## 7. Entity Component System (ECS) as Idiom

Datara supports Entity Component Systems through value-typed `record` structures for cache-locality and pure systems for predictable mutations:

```datara
record PositionComponent {
    x: Float,
    y: Float,
    z: Float,
}

record VelocityComponent {
    vx: Float,
    vy: Float,
    vz: Float,
}

record Actor {
    id: Int,
    pos: PositionComponent,
    vel: VelocityComponent,
}

fn movement_system(a: Actor, dt: Float) -> Actor {
    let new_pos = PositionComponent {
        x: a.pos.x + a.vel.vx * dt,
        y: a.pos.y + a.vel.vy * dt,
        z: a.pos.z + a.vel.vz * dt,
    }
    return Actor { id: a.id, pos: new_pos, vel: a.vel }
}

fn main() {
    let p = PositionComponent { x: 0.0, y: 0.0, z: 0.0 }
    let v = VelocityComponent { vx: 5.0, vy: 0.0, vz: -2.0 }
    mut actor = Actor { id: 1, pos: p, vel: v }
    actor = movement_system(actor, 0.01666667)
    out actor.pos.x
}
```

---

## 8. Scope Boundary: Input & Windowing Limitation

> [!IMPORTANT]
> **Minimal Honest Gamedev Layer**: Datara v1.1 deliberately focuses on the deterministic computational core (fixed timestep loop, accumulator, SIMD physics, frame arena, and ECS data structures).
>
> Native OS windowing, swapchains, and direct keyboard/mouse/gamepad polling (e.g. via SDL2, winit, Win32, or Wayland) are intentionally excluded from the core compiler distribution at this stage. Windowing and graphics presentation will be delivered in a separate, dedicated ecosystem release once multi-platform C/Rust FFI packaging is fully automated through Sparks.

---

## 9. Verification and Showcases

- `examples/showcase/game_loop/`: Complete deterministic 60 Hz game loop with accumulator and ECS.
- `examples/showcase/lockstep_sim/`: Multi-core lockstep simulation with SIMD physics.
- `tests/test_gamedev_phase3.rs`: Verifies that every single documentation example compiles cleanly, validates time API monotonicity, and checks 20-run bit-for-bit game loop determinism.
