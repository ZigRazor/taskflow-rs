# TaskFlow-RS Design Document

## Architecture Overview

TaskFlow-RS is a task-parallel programming library built around directed acyclic graphs (DAGs) of closures. Each node in the graph is a task; edges encode happens-before relationships. An executor drives the graph to completion using a work-stealing thread pool.

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

### Low Priority

- [x] **GPU Support** ✅ COMPLETED
  - [x] CUDA device integration via cudarc
  - [x] `GpuBuffer<T>` typed device buffer management
  - [x] Synchronous host↔device transfers (`copy_from_host` / `copy_to_host`)
  - [x] `GpuTaskConfig` launch dimensions (1D and 2D)
  - [x] Heterogeneous task graphs (CPU + GPU tasks in same DAG)
  - [x] **Asynchronous transfers** ✅ COMPLETED
    - [x] `copy_from_host_async` / `copy_to_host_async` (unsafe, stream-scoped)
    - [x] `copy_from_host_async_owned` / `copy_to_host_async_owned` (safe, Tokio)
    - [x] `AsyncTransferBuilder` fluent pipeline API
  - [x] **Multiple CUDA streams** ✅ COMPLETED
    - [x] `GpuStream` — named per-device command queue
    - [x] `StreamPool` — N-stream pool with round-robin and least-pending strategies
    - [x] `StreamGuard` — RAII borrow with automatic pending-op accounting
    - [x] `StreamSet` — fixed-depth stream set for double-buffer pipelines
    - [x] `GpuDevice::stream_pool` / `stream_set` / `create_stream` factory methods
  - [x] **OpenCL/ROCm backends** ✅ COMPLETED
    - [x] `ComputeBackend` trait — pluggable backend abstraction
    - [x] `BackendKind` enum — CUDA / OpenCL / ROCm / Stub
    - [x] `probe_backend` — auto-selects best available backend at runtime
    - [x] CUDA backend (`gpu_cuda_backend.rs`) — wraps cudarc 0.11
    - [x] OpenCL backend (`gpu_opencl.rs`) — wraps opencl3 0.9, works on NVIDIA/AMD/Intel
    - [x] ROCm/HIP backend (`gpu_rocm.rs`) — raw HIP FFI via `libamdhip64`
    - [x] Stub backend — always available, used in CI / no-GPU environments
    - [x] `GpuDevice::with_backend(id, BackendKind)` explicit backend selection
  - [ ] Unified / pinned memory (planned)
  - [ ] Automatic kernel generation (planned)

- [x] **Advanced Features** ✅ COMPLETED
  - [x] Task priorities (`Priority` enum: Low / Normal / High / Critical)
  - [x] Cooperative task cancellation (`CancellationToken`)
  - [x] Custom scheduler trait (`FifoScheduler`, `PriorityScheduler`, `RoundRobinScheduler`)
  - [x] NUMA topology detection and worker pinning
  - [ ] Preemptive cancellation (planned)
  - [ ] Dynamic priority adjustment (planned)
  - [ ] Hardware topology integration with hwloc (planned)

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

### Pluggable GPU Backend (`ComputeBackend` trait)

The GPU layer is built around a single trait:

```rust
pub trait ComputeBackend: Send + Sync + fmt::Debug + 'static {
    fn kind(&self) -> BackendKind;
    fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError>;
    fn htod_sync(&self, src: *const c_void, bytes: usize, dst: &DeviceBuffer) -> Result<(), GpuError>;
    fn dtoh_sync(&self, src: &DeviceBuffer, dst: *mut c_void, bytes: usize) -> Result<(), GpuError>;
    unsafe fn htod_async(..., stream: &DeviceStream) -> Result<(), GpuError>;
    unsafe fn dtoh_async(..., stream: &DeviceStream) -> Result<(), GpuError>;
    fn create_stream(&self) -> Result<DeviceStream, GpuError>;
    fn synchronize_device(&self) -> Result<(), GpuError>;
    fn memory_info(&self) -> Result<(usize, usize), GpuError>;
}
```

**Why trait-object dispatch over generics?**
- Backends are selected at **runtime** via `probe_backend()` (CUDA → ROCm → OpenCL → Stub)
- The vtable cost is negligible compared to any real GPU operation
- User code is backend-agnostic; switching hardware requires changing one line

**Downcast pattern:** `BackendBuffer` and `BackendStream` both expose `fn as_any(&self) -> &dyn Any`, enabling safe concrete-type recovery inside each backend without exposing implementation details through the trait.

### Async Transfers

Two levels of async transfer are provided:

1. **`unsafe` stream-scoped** (`copy_from_host_async`): enqueues work on a `GpuStream`, returns immediately. Caller must guarantee the host slice outlives `stream.synchronize()`. Used when lifetimes can be proven at the call site.

2. **Safe owned** (`copy_from_host_async_owned`): takes ownership of the `Vec<T>`, runs the blocking copy on `tokio::task::spawn_blocking`, returns `(GpuBuffer, Vec<T>)`. No lifetime juggling. Used in Tokio async contexts.

### StreamPool Assignment Strategies

`StreamPool` supports two assignment policies selected at construction time:

- **`RoundRobin`**: streams handed out in fixed rotation. Constant time, zero contention, good for uniform workloads.
- **`LeastPending`**: streams tracked by `AtomicU64` pending-op counter; the pool hands out the least-loaded stream. Better for uneven batch sizes.

`StreamGuard` decrements the counter on drop (RAII), keeping accounting automatic.

### `Arc<Mutex<Vec<TaskNode>>>` for the Task Graph

**Pros:** simple, thread-safe shared ownership, works with Rust ownership.  
**Cons:** mutex contention under many workers.  
**Alternatives:** lock-free structures (crossbeam), per-worker queues, immutable graphs with runtime state.  
The current design prioritises correctness and maintainability over maximum throughput; the work-stealing executor already minimises graph contention in practice.

### Work Stealing vs Work Sharing

**Implemented: Work stealing ✅**

Each worker has its own lock-free deque (`crossbeam::deque`):
- Workers push/pop from their own queue (LIFO — better cache locality)
- Idle workers steal from the front of others' queues (FIFO)
- Near-linear scalability on multi-core systems

### Feature Flags

| Flag | Enables |
|------|---------|
| `gpu` | CUDA backend via cudarc (NVIDIA) |
| `opencl` | OpenCL backend via opencl3 (NVIDIA/AMD/Intel) |
| `rocm` | ROCm/HIP backend via raw FFI (AMD) |
| `all-gpu` | All three GPU backends simultaneously |
| `async` | Tokio async executor and async task variants |

The `Stub` backend requires no flag and is always available — used in CI, unit tests, and environments without GPU hardware.
