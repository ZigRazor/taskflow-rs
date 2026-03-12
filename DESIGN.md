# TaskFlow-RS Design Document

## Architecture Overview

TaskFlow-RS is a task-parallel programming library built around directed acyclic graphs (DAGs) of
closures. Each node in the graph is a task; edges encode happens-before relationships. An executor
drives the graph to completion using a work-stealing thread pool. Three scheduling subsystems layer
on top of the core executor: a static priority queue, a dynamic reprioritization engine, and a
hardware-topology-aware affinity layer.

```
┌──────────────────────────────────────────────────────────────┐
│                        User Code                              │
│   Taskflow (DAG)  ·  Pipeline  ·  Composition  ·  Async      │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────────┐
│                   Scheduling Layer                             │
│  SharedDynamicScheduler  ·  EscalationPolicy                  │
│  PriorityScheduler  ·  FifoScheduler  ·  RoundRobinScheduler  │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────────┐
│                    Executor (work-stealing)                    │
│  Per-worker lock-free deques  ·  Dependency counters          │
│  PreemptiveCancellationToken  ·  DeadlineGuard                │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────────┐
│              Hardware Topology Layer                           │
│  TopologyProvider (hwloc2 / sysfs fallback)                   │
│  HwlocWorkerAffinity  ·  AffinityStrategy  ·  CacheInfo       │
└──────────────────────────────────────────────────────────────┘
```

---

## Implementation Status

### Core

- [x] **Task Graph** ✅ COMPLETED
  - [x] DAG construction with `emplace` / `precede` / `succeed`
  - [x] Cycle detection
  - [x] Task naming and metadata
  - [x] Subflows (nested task graphs)
  - [x] Condition tasks (conditional branching)
  - [x] Loop constructs

- [x] **Executor** ✅ COMPLETED
  - [x] Work-stealing scheduler (crossbeam deque)
  - [x] Per-worker lock-free queues
  - [x] Configurable worker count (0 = auto)
  - [x] `run`, `run_n`, `run_until`, `run_many` variants
  - [x] Parallel `run_n` (N instances concurrently)
  - [x] `wait_for_all` barrier

- [x] **Parallel Algorithms** ✅ COMPLETED
  - [x] `parallel_for_each`
  - [x] `parallel_reduce`
  - [x] `parallel_transform`
  - [x] `parallel_sort`
  - [x] `parallel_inclusive_scan` / `parallel_exclusive_scan`

### Medium Priority

- [x] **Async Support** ✅ COMPLETED
  - [x] `AsyncExecutor` with Tokio integration
  - [x] `emplace_async` for `async fn` tasks
  - [x] `run_n_async`, `run_n_sequential_async`, `run_until_async`
  - [x] Shared state support with `Arc`/`Mutex`

- [x] **Pipeline** ✅ COMPLETED
  - [x] `ConcurrentPipeline` with token management
  - [x] Serial and parallel stages
  - [x] Backpressure / flow control
  - [x] `TypeSafePipeline` with compile-time type checking
  - [x] `SimplePipeline` for in-place mutations

- [x] **Composition** ✅ COMPLETED
  - [x] `Composition` / `CompositionBuilder` with entry/exit points
  - [x] `CloneableWork` for Arc-based task sharing
  - [x] `ParameterizedComposition` factory pattern
  - [x] `CompositionParams` with typed parameters (Int, Float, String, Bool)

- [x] **Run Variants** ✅ COMPLETED
  - [x] `run_n` — N sequential instances
  - [x] `run_until` — run until predicate
  - [x] `run_many` / `run_many_and_wait` — concurrent taskflows
  - [x] Thread-based parallel `run_n`

- [x] **Built-in Metrics** ✅ COMPLETED
  - [x] `Metrics` system with task execution tracking
  - [x] Worker utilization, tasks/sec, timing histogram
  - [x] Memory usage and success/failure rates

### Advanced Scheduling

- [x] **Task Priorities** ✅ COMPLETED
  - [x] `Priority` enum: `Low` / `Normal` / `High` / `Critical`
  - [x] Static priority queue (`PriorityScheduler`) with FIFO tie-breaking
  - [x] `FifoScheduler` and `RoundRobinScheduler` built-ins
  - [x] Pluggable `Scheduler` trait for custom strategies

- [x] **Cooperative Task Cancellation** ✅ COMPLETED
  - [x] `CancellationToken` — shared atomic flag, cheaply cloneable
  - [x] `is_cancelled()` / `cancel()` / `reset()` / `cancel_count()`

- [x] **Preemptive Cancellation** ✅ COMPLETED
  - [x] `PreemptiveCancellationToken` — extends cooperative model
  - [x] `cancel_after(Duration)` — watchdog thread fires at deadline
  - [x] `cancel_at(Instant)` — absolute-time deadline
  - [x] `cancel_with(reason)` — attaches a human-readable reason
  - [x] `check()` → `Result<(), Preempted>` — ergonomic `?` integration
  - [x] `check_and_yield()` — combines cancellation check with `yield_now()`
  - [x] `DeadlineGuard` — RAII scope-bound time budget
  - [x] `with_deadline(budget, closure)` — scoped deadline helper
  - [x] `reset()` — token reuse across multiple task runs
  - [x] Signal-based preemption via SIGUSR2 (Linux, opt-in)
  - [x] Watchdog self-deadlock-safe implementation (guard dropped before notify)

- [x] **Dynamic Priority Adjustment** ✅ COMPLETED
  - [x] `DynamicPriorityScheduler` — O(log n) reprioritization via dual-index
  - [x] `SharedDynamicScheduler` — thread-safe `Arc<Mutex<_>>` wrapper
  - [x] `PriorityHandle` — `Weak`-backed handle for deferred reprioritization
  - [x] `reprioritize(task_id, new_priority)` — live priority change
  - [x] FIFO ordering preserved within equal-priority tasks (sequence numbers)
  - [x] `remove(task_id)` — pre-execution cancellation via handle
  - [x] `snapshot()` — non-destructive priority-ordered view
  - [x] `EscalationPolicy` — tick-driven anti-starvation for Low / Normal tasks
  - [x] Implements `Scheduler` trait for drop-in executor integration

- [x] **Hardware Topology Integration** ✅ COMPLETED
  - [x] `TopologyProvider` — auto-selects hwloc2 or sysfs fallback at runtime
  - [x] `HwTopology` trait — backend-agnostic topology interface
  - [x] `HwlocBackend` (feature `hwloc`) — wraps `hwloc2 = "2.2.0"`
  - [x] `SysfsBackend` (always available) — reads `/sys/devices/system/cpu/*/cache/`
  - [x] NUMA node discovery with per-node memory capacity
  - [x] CPU package / socket enumeration
  - [x] L1 / L2 / L3 cache hierarchy: size, line size, associativity, shared CPUs
  - [x] `bind_thread(&[cpu])` — calls `pthread_setaffinity_np` (Linux) or hwloc bind
  - [x] `numa_node_for_cpu(cpu)` — O(1) CPU → NUMA node mapping
  - [x] `HwlocWorkerAffinity` — maps worker IDs to CPU sets
  - [x] Five `AffinityStrategy` variants: `None`, `NUMARoundRobin`, `NUMADense`,
        `PhysicalCores`, `L3CacheDomain`

- [x] **NUMA-Aware Scheduling** ✅ COMPLETED
  - [x] `NumaTopology::detect()` via sysfs
  - [x] `NumaPinning` strategies: None / RoundRobin / Dense / Sparse
  - [x] `get_worker_cpus(worker_id, num_workers, topology, strategy)`
  - [x] `TaskMetadata::with_numa_node(id)` hint

### Low Priority

- [x] **GPU Support** ✅ COMPLETED
  - [x] CUDA device integration via cudarc
  - [x] `GpuBuffer<T>` typed device buffer management
  - [x] Synchronous host↔device transfers
  - [x] Asynchronous transfers with `GpuStream` and `StreamPool`
  - [x] Multiple CUDA streams with `StreamSet` and `StreamGuard`
  - [x] OpenCL backend (`gpu_opencl.rs`) — NVIDIA / AMD / Intel
  - [x] ROCm/HIP backend (`gpu_rocm.rs`) — raw HIP FFI
  - [x] Stub backend — CI / no-GPU environments
  - [ ] Unified / pinned memory (planned)
  - [ ] Automatic kernel generation (planned)

- [x] **Tooling** ✅ COMPLETED
  - [x] `Profiler` with `ExecutionProfile` statistics
  - [x] DOT graph export, SVG timeline, HTML report
  - [x] Real-time `PerformanceMetrics` and worker utilization
  - [x] `DebugLogger` with structured log levels
  - [ ] Real-time dashboard (planned)
  - [ ] Flamegraph generation (planned)
  - [ ] Automated regression detection (planned)

---

## Design Decisions

### Preemptive Cancellation: Three-Layer Model

True CPU preemption is unsafe in Rust's ownership model, so preemptive cancellation is
implemented as three escalating layers with the same token type:

```
Layer 1 — Cooperative (zero overhead on the fast path)
  task loop: token.check()?   →  Err(Preempted) if flag set

Layer 2 — Watchdog (background thread, ~1 ms granularity)
  token.cancel_after(Duration)  →  spawns watchdog thread
  token.cancel_at(Instant)      →  absolute deadline

Layer 3 — Signal (Linux only, microsecond latency, opt-in)
  PreemptiveCancellationToken::install_signal_handler()  (once at startup)
  token.signal_preempt_thread(pthread_id)                 →  SIGUSR2
  token.check_signal()                                    →  thread-local flag
```

**Watchdog self-deadlock fix:** `Condvar::wait_timeout` re-acquires the `wakeup` mutex on
return. The watchdog must `drop(guard)` that re-acquired lock *before* calling
`cancel_with_reason`, which itself locks `wakeup` to notify waiters. Holding the lock across
the call causes the thread to block on a mutex it already owns. The fix:

```rust
let (guard, timed_out) = state.condvar.wait_timeout(lock, sleep_for).unwrap();
let should_cancel = timed_out.timed_out() && !state.cancelled.load(Ordering::Acquire);
drop(guard);          // ← release BEFORE cancel_with_reason
if should_cancel {
    state.cancel_with_reason(reason);
}
```

**`DeadlineGuard` RAII:** scope-bound budgets that cancel automatically on drop:

```rust
let _guard = token.deadline_guard(Duration::from_millis(100));
// ...task body...
// guard drops here: if elapsed > 100 ms, token is cancelled
```

### Dynamic Priority Adjustment: Dual-Index Data Structure

Standard `BinaryHeap` does not support O(log n) key updates. The scheduler maintains two
parallel data structures:

```
index:   BTreeMap<(RevPriority, SeqNum), TaskId>   ← ordered pop:     O(log n)
reverse: HashMap<TaskId, (RevPriority, SeqNum)>    ← lookup by id:    O(1)
```

`reprioritize(task_id, new_priority)`:
1. `reverse.remove(task_id)` → retrieves `(old_rp, old_seq)`  — O(1)
2. `index.remove((old_rp, old_seq))`  — O(log n)
3. `index.insert((new_rp, old_seq), task_id)` — O(log n)
4. `reverse.insert(task_id, (new_rp, old_seq))` — O(1)

**Sequence number invariant:** the sequence number assigned at push time is preserved through
all reprioritizations. This guarantees that among tasks sharing the same priority, order of
insertion is always respected (FIFO), even after an escalation or demotion.

**`RevPriority` newtype:** `BTreeMap` sorts ascending; higher-numeric priorities should sort
first. `RevPriority(u8)` stores `u8::MAX - priority_value`, inverting the sort order with
zero runtime cost.

**`PriorityHandle` with `Weak` reference:** handles returned by `push()` hold only a
`Weak<Mutex<DynamicPriorityScheduler>>`, so they do not prevent the scheduler from being
dropped and cannot cause reference cycles. `reprioritize()` returns `false` gracefully when
either the scheduler or the task is already gone.

**Anti-starvation with `EscalationPolicy`:** a tick counter drives periodic escalation passes
that bump `Low → Normal` and `Normal → High`. Callers invoke `policy.tick()` each scheduler
loop iteration. The tick interval and age thresholds are configurable; `escalate()` can also
be called directly for immediate passes.

### Hardware Topology: Backend-Trait Abstraction

The topology layer uses a trait object to provide a single API regardless of whether hwloc is
available at compile time:

```rust
pub trait HwTopology: Send + Sync {
    fn numa_nodes(&self)  -> &[NumaNode];
    fn cpu_count(&self)   -> usize;
    fn packages(&self)    -> Vec<PackageInfo>;
    fn cache_info(&self)  -> Vec<CacheInfo>;
    fn bind_thread(&self, cpus: &[usize]) -> Result<(), BindError>;
    fn numa_node_for_cpu(&self, cpu: usize) -> Option<usize>;
    fn is_hwloc_backed(&self) -> bool;
    fn backend_name(&self)   -> &'static str;
}
```

`TopologyProvider::detect()` selects the backend at startup:

```
--features hwloc  →  tries HwlocBackend (hwloc2 = "2.2.0")
                      falls back to SysfsBackend on hwloc init failure
no feature flag   →  SysfsBackend always (zero extra system dependency)
```

**hwloc2 2.2.0 API notes:**
- `CpuBindFlags::THREAD` (not `CPUBIND_THREAD`) — binds the calling OS thread
- Cache attributes via `obj.cache_attributes()` → `TopologyCacheAttributes`
  (`.size()`, `.line_size()`, `.associativity()`)
- Memory via `obj.memory().local_memory()` (not `obj.attr().numanode()`)
- `CpuSet` (= `Bitmap`) iteration: `cpuset.iter()` yields `u32` indices

**SysfsBackend** reads `/sys/devices/system/cpu/cpuN/cache/indexM/{level,size,coherency_line_size,shared_cpu_list}` on Linux, deduplicating cache entries by selecting only the instance where the current CPU equals `min(shared_cpus)`. Falls back to a single synthetic L3 entry on non-Linux or when sysfs is unavailable.

**`HwlocWorkerAffinity`** maps each worker ID to a CPU set using one of five strategies:

| Strategy | Description |
|---|---|
| `None` | No binding; OS scheduler decides |
| `NUMARoundRobin` | Distribute workers evenly across NUMA nodes |
| `NUMADense` | Fill each NUMA node before moving to the next |
| `PhysicalCores` | Pin to physical cores only (skip HT siblings, heuristic) |
| `L3CacheDomain` | Assign workers to L3 cache sharing groups |

### Pluggable GPU Backend (`ComputeBackend` trait)

The GPU layer is built around a single trait with runtime dispatch:

```rust
pub trait ComputeBackend: Send + Sync + fmt::Debug + 'static {
    fn kind(&self) -> BackendKind;
    fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError>;
    fn htod_sync(&self, ...) -> Result<(), GpuError>;
    fn dtoh_sync(&self, ...) -> Result<(), GpuError>;
    unsafe fn htod_async(&self, ..., stream: &DeviceStream) -> Result<(), GpuError>;
    unsafe fn dtoh_async(&self, ..., stream: &DeviceStream) -> Result<(), GpuError>;
    fn create_stream(&self) -> Result<DeviceStream, GpuError>;
    fn synchronize_device(&self) -> Result<(), GpuError>;
    fn memory_info(&self) -> Result<(usize, usize), GpuError>;
}
```

Trait-object dispatch is chosen over monomorphization because backends are selected at runtime
via `probe_backend()` (CUDA → ROCm → OpenCL → Stub), and the vtable cost is negligible
compared to any GPU operation.

### Async Transfers

Two levels of async GPU transfer:

1. **`unsafe` stream-scoped** (`copy_from_host_async`): enqueues work on a `GpuStream`,
   returns immediately. Caller guarantees the host slice outlives `stream.synchronize()`.
2. **Safe owned** (`copy_from_host_async_owned`): takes ownership of `Vec<T>`, runs on
   `tokio::task::spawn_blocking`, returns `(GpuBuffer, Vec<T>)`.

---

## Module Structure

```
src/
├── lib.rs                  — Public API, re-exports, feature gates
├── executor.rs             — Work-stealing thread pool
├── taskflow.rs             — DAG construction
├── task.rs                 — Task nodes, handles, dependency tracking
├── scheduler.rs            — Scheduler trait, FifoScheduler, PriorityScheduler,
│                             RoundRobinScheduler
│
├── advanced.rs             — Priority enum, CancellationToken, TaskMetadata
├── preemptive.rs           — PreemptiveCancellationToken, DeadlineGuard,
│                             with_deadline, signal preemption (Linux)
├── dynamic_priority.rs     — DynamicPriorityScheduler, SharedDynamicScheduler,
│                             PriorityHandle, EscalationPolicy
├── hwloc_topology.rs       — TopologyProvider, HwTopology trait, HwlocBackend,
│                             SysfsBackend, HwlocWorkerAffinity, AffinityStrategy
├── numa.rs                 — NumaTopology, NumaNode, NumaPinning (sysfs-level)
│
├── subflow.rs              — Nested task graphs
├── condition.rs            — Conditional branching, Loop
├── pipeline.rs             — ConcurrentPipeline, stage types
├── typed_pipeline.rs       — TypeSafePipeline, PipelineBuilder
├── composition.rs          — Composition, CompositionBuilder, parameterization
├── algorithms.rs           — Parallel for_each, reduce, transform, sort, scan
│
├── gpu.rs                  — GpuDevice, GpuBuffer<T>, GpuTaskConfig (public API)
├── gpu_backend.rs          — ComputeBackend trait, BackendKind, DeviceBuffer
├── gpu_stream.rs           — GpuStream, StreamPool, StreamSet, StreamGuard
├── gpu_cuda_backend.rs     — CUDA backend (feature = "gpu")
├── gpu_opencl.rs           — OpenCL backend (feature = "opencl")
├── gpu_rocm.rs             — ROCm/HIP backend (feature = "rocm")
│
├── profiler.rs             — Profiler, ExecutionProfile, TaskStats
├── visualization.rs        — DOT / SVG / HTML export
├── monitoring.rs           — PerformanceMetrics, worker utilization
├── metrics.rs              — Metrics, MetricsSummary
├── debug.rs                — DebugLogger, LogLevel, LogEntry
│
├── async_executor.rs       — AsyncExecutor (feature = "async")
├── cycle_detection.rs      — CycleDetector
└── future.rs               — TaskflowFuture
```

---

## Concurrency and Safety Guarantees

| Component | Synchronization | Notes |
|---|---|---|
| `PreemptiveCancellationToken` | `Arc<AtomicBool>` + `Condvar` | Clone is O(1), check is one `Acquire` load |
| `SharedDynamicScheduler` | `Arc<Mutex<_>>` | Lock held only during push/pop/reprioritize |
| `PriorityHandle` | `Weak<Mutex<_>>` | No strong reference; scheduler can be dropped freely |
| `EscalationPolicy` | single-threaded tick | Intended for the scheduler loop thread only |
| `TopologyProvider` | immutable after `detect()` | `bind_thread` is per-thread, no shared state |
| Work-stealing deques | `crossbeam::deque` | Lock-free; LIFO own-queue, FIFO steal |
| Task dependency counters | `AtomicUsize` | `Release` on decrement, `Acquire` on zero-check |

---

## Performance Characteristics

| Operation | Complexity | Notes |
|---|---|---|
| `DynamicPriorityScheduler::push` | O(log n) | BTreeMap insert + HashMap insert |
| `DynamicPriorityScheduler::pop` | O(log n) | BTreeMap first entry removal |
| `DynamicPriorityScheduler::reprioritize` | O(log n) | Two BTree ops, two HashMap ops |
| `DynamicPriorityScheduler::remove` | O(log n) | Same as reprioritize |
| `PreemptiveCancellationToken::check` | O(1) | Single `Acquire` atomic load |
| `PreemptiveCancellationToken::cancel_after` | O(1) | Spawns one background thread |
| `TopologyProvider::detect` | one-time | Amortized across all workers |
| `HwlocWorkerAffinity::cpus_for_worker` | O(nodes) | Proportional to NUMA node count |
| `bind_thread` (Linux) | O(cpus) | One `pthread_setaffinity_np` syscall |

---

## Future Enhancements

- Priority inheritance for dependent tasks (Critical task promotes its blockers)
- Real-time scheduling support (SCHED_FIFO / SCHED_RR integration)
- Automatic NUMA memory allocation alignment with worker topology
- Signal-based preemption on macOS (thread port messaging)
- Per-cache-domain work queues for L3-local task stealing
- Unified / pinned GPU memory
- Automatic GPU kernel generation
- Real-time performance dashboard
- Flamegraph generation
- Automated performance regression detection
