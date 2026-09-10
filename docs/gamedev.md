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
// Fast vector clamped acceleration
let bound_min = float4(-100.0, -100.0, -100.0, 0.0)
let bound_max = float4(100.0, 100.0, 100.0, 10.0)

let clamped_pos = min4(max4(pos, bound_min), bound_max)
let energy = dot(clamped_pos, clamped_pos)
```

---

## 3. Frame Arena Allocator (Zero-Allocation Per Frame)

Allocating on the general heap (`malloc`/`free`) during the inner loop of a 60/120 FPS game introduces cache thrashing and fragmentation. Datara exposes a linear thread-local **Frame Arena Allocator**:

### Runtime API
- `datara_rt_arena_alloc(size: Int) -> Pointer`: Allocates contiguous bytes in constant time $O(1)$ by bumping an internal pointer.
- `datara_rt_arena_checkpoint() -> Int`: Returns the current arena offset mark.
- `datara_rt_arena_reset(checkpoint: Int)`: Rewinds the arena offset back to the checkpoint in $O(1)$, freeing all temporary allocations made in the frame.

### Game Loop Pattern
```datara
fn game_frame() {
    let cp = datara_rt_arena_checkpoint()

    // Transient allocations during AI, spatial queries, and collision broadphase
    run_physics_broadphase()
    run_particle_simulation()

    // Free everything allocated in this frame with zero overhead
    datara_rt_arena_reset(cp)
}
```

---

## 4. Multi-Core Simulation with `parallel for`

Workloads such as particle simulations, flocking/boids, and entity updates can be parallelized with `parallel for`:

```datara
fn run_worker(worker_id: Int) {
    let entities_per_worker = 16
    let start_id = worker_id * entities_per_worker
    let end_id = start_id + entities_per_worker
    mut i = start_id
    while i < end_id {
        simulate_entity(i, 50)
        i = i + 1
    }
}

fn update_world() {
    let num_workers = 4
    parallel for w in 0..num_workers {
        run_worker(w)
    }
}
```

---

## 5. Struct-Based Entity Component System (ECS)

Datara structs are contiguous, cache-friendly value types suited for Data-Oriented Design (SoA - Structure of Arrays and AoS - Array of Structures):

```datara
struct Transform {
    x: Float,
    y: Float,
    z: Float,
    rot: Float
}

struct RigidBody {
    vx: Float,
    vy: Float,
    vz: Float,
    mass: Float
}

struct World {
    transforms: List<Transform>,
    bodies: List<RigidBody>
}
```

---

## 6. Verification and Showcase

The complete lockstep simulation showcase is located in `examples/showcase/lockstep_sim/`:
- `datara.toml`: Project manifest.
- `lockstep.dtr`: Full deterministic simulation with multi-threaded workers and SIMD.
- Integration test: `tests/test_lockstep_sim.rs` verifies that 4 distinct runs across multi-core threads produce bit-for-bit identical 64-bit physics checksums.
