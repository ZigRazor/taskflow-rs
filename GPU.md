# GPU Support (v2)

TaskFlow-RS provides a **backend-agnostic GPU API** with support for CUDA, OpenCL, and ROCm/HIP.  All three share the same user-facing types (`GpuDevice`, `GpuBuffer`, `GpuStream`) — switching backends requires changing only one line.

---

## Feature Matrix

| Feature | CUDA | OpenCL | ROCm/HIP | Stub |
|---------|:----:|:------:|:--------:|:----:|
| Buffer alloc/free | ✅ | ✅ | ✅ | ✅ |
| Sync H2D / D2H | ✅ | ✅ | ✅ | ✅ |
| **Async H2D / D2H** | ✅ | ✅ | ✅ | ✅ |
| **Multiple streams** | ✅ | ✅ | ✅ | ✅ |
| **StreamPool** | ✅ | ✅ | ✅ | ✅ |
| **Double-buffer pipeline** | ✅ | ✅ | ✅ | ✅ |
| Kernel launch | ✅ | Planned | Planned | — |
| Unified memory | Planned | — | — | — |
| Memory info query | ✅ | ✅ | ✅ | ✅ |

---

## Building

### CUDA (NVIDIA)

```bash
# Default: CUDA 12.0
cargo build --features gpu

# Different CUDA version (edit Cargo.toml gpu feature line):
# gpu = ["cudarc", "cudarc/cuda-11080"]  → CUDA 11.8
# gpu = ["cudarc", "cudarc/cuda-12050"]  → CUDA 12.5
```

### OpenCL (NVIDIA / AMD / Intel / Apple)

```bash
# Requires an OpenCL ICD loader and at least one OpenCL driver
# Ubuntu: sudo apt install ocl-icd-opencl-dev opencl-headers
# Fedora: sudo dnf install ocl-icd-devel  opencl-headers
cargo build --features opencl
```

### ROCm / HIP (AMD)

```bash
# Requires ROCm ≥ 5.0: https://rocm.docs.amd.com
export ROCM_PATH=/opt/rocm
cargo build --features rocm
```

### All backends

```bash
cargo build --features all-gpu
```

### No GPU (Stub — for CI / dev machines)

```bash
cargo build   # stub backend is always available, no extra flags
```

---

## Core Concepts

### GpuDevice

Backend-agnostic device handle.  Auto-selects the best available backend:

```rust
// Auto-select: CUDA → ROCm → OpenCL → Stub
let device = GpuDevice::new(0)?;

// Force a specific backend
let device = GpuDevice::with_backend(0, BackendKind::OpenCL)?;
let device = GpuDevice::with_backend(0, BackendKind::Rocm)?;
let device = GpuDevice::with_backend(0, BackendKind::Stub)?;

println!("{} ({:?})", device.name(), device.backend_kind());
let (free, total) = device.memory_info()?;
```

### GpuBuffer\<T\>

Typed device buffer.  Sync and async transfers:

```rust
let mut buf: GpuBuffer<f32> = GpuBuffer::allocate(&device, 1024)?;

// ── Synchronous (blocking) ───────────────────────────────────────────────
let src = vec![1.0f32; 1024];
buf.copy_from_host(&src)?;

let mut dst = vec![0.0f32; 1024];
buf.copy_to_host(&mut dst)?;

// ── Asynchronous (enqueued on a stream) ───────────────────────────────────
let stream = device.create_stream("my-stream")?;

unsafe {
    // Returns immediately; copy executes when stream is scheduled
    buf.copy_from_host_async(&src, &stream)?;
    buf.copy_to_host_async(&mut dst, &stream)?;
}

// Wait for the stream's work to complete
stream.synchronize()?;
```

---

## Multiple CUDA Streams

### StreamPool — managed pool with automatic assignment

```rust
// Round-robin pool of 4 streams
let pool = device.stream_pool(4)?;

// Least-loaded pool (picks stream with fewest pending ops)
let pool = device.stream_pool_with(4, StreamAssignment::LeastPending)?;

// Acquire a stream from the pool (RAII guard)
{
    let guard = pool.acquire()?;
    let stream = guard.stream();

    unsafe { buf.copy_from_host_async(&data, stream)?; }

    // Guard dropped here — decrements pending-op counter
}

// Barrier: wait for all streams in the pool
pool.synchronize_all()?;
```

### StreamSet — fixed-depth pipeline

```rust
// 3-deep pipeline: streams 0, 1, 2 cycle round-robin
let set = device.stream_set(3, "compute")?;

for (i, batch) in batches.iter().enumerate() {
    // Sync the slot before reusing it
    if i >= 3 {
        set.get(i).synchronize()?;  // waits for stream i%3
    }

    let stream = set.get(i);       // i % 3
    unsafe { buf.copy_from_host_async(batch, stream)?; }
    // launch_kernel(&buf, stream)?;
}
set.synchronize_all()?;
```

### Manual stream creation

```rust
let s0 = device.create_stream("h2d-0")?;
let s1 = device.create_stream("compute-0")?;
let s2 = device.create_stream("d2h-0")?;

unsafe {
    buf_a.copy_from_host_async(&input_a, &s0)?;
    buf_b.copy_from_host_async(&input_b, &s0)?;  // s0: serial H2D
    // kernel on s1 (overlaps with H2D on s0)
    buf_out.copy_to_host_async(&mut result, &s2)?;
}

s0.synchronize()?;
s1.synchronize()?;
s2.synchronize()?;
```

---

## Async Transfers (Rust async/await)

For ergonomic integration with Tokio task graphs:

```rust
// Takes ownership of the Vec to guarantee lifetime safety
let (buf, data) = buf.copy_from_host_async_owned(data).await?;
let (buf, result) = buf.copy_to_host_async_owned(result).await?;
```

This uses `tokio::task::spawn_blocking` internally so the async runtime is
never blocked by GPU operations.

---

## OpenCL / ROCm Backends

The same user-facing code works across all backends:

```rust
// CUDA
let device = GpuDevice::with_backend(0, BackendKind::Cuda)?;

// OpenCL — same code, different backend
let device = GpuDevice::with_backend(0, BackendKind::OpenCL)?;

// ROCm — same code, different backend
let device = GpuDevice::with_backend(0, BackendKind::Rocm)?;

// Everything below is identical regardless of backend
let pool = device.stream_pool(4)?;
let mut buf: GpuBuffer<f32> = GpuBuffer::allocate(&device, N)?;
buf.copy_from_host(&data)?;
```

### OpenCL notes

- Each "stream" is an independent `CommandQueue`.
- `htod_async` / `dtoh_async` use `CL_FALSE` (non-blocking enqueue).
- `stream.synchronize()` calls `clFinish()`.
- Works with NVIDIA CUDA driver (OpenCL 3.0), AMD ROCm, Intel oneAPI.

### ROCm/HIP notes

- The HIP API mirrors CUDA; streams map 1-to-1 to `hipStream_t`.
- Async transfers use `hipMemcpyAsync`.
- Set `ROCM_PATH=/opt/rocm` before building.
- Set `LD_LIBRARY_PATH=$ROCM_PATH/lib` before running.

---

## Multi-Stream in a TaskFlow DAG

```rust
let device = Arc::new(GpuDevice::new(0)?);
let pool   = Arc::new(device.stream_pool(4)?);

let mut taskflow = Taskflow::new();

// Each GPU task pulls a stream from the shared pool
let tasks: Vec<_> = (0..4).map(|i| {
    let pool_ref   = Arc::clone(&pool);
    let device_ref = Arc::clone(&device);

    taskflow.emplace(move || {
        let guard  = pool_ref.acquire().unwrap();
        let stream = guard.stream();

        let mut buf = GpuBuffer::<f32>::allocate(&device_ref, 1024).unwrap();
        unsafe {
            buf.copy_from_host_async(&input[i], stream).unwrap();
        }
        stream.synchronize().unwrap();
    })
}).collect();

// Wire up DAG as needed ...
executor.run(&taskflow).wait();

// Final barrier across all streams
pool.synchronize_all()?;
```

---

## Double-Buffer Pipeline Pattern

Overlap CPU data preparation with GPU computation:

```
Time →

CPU:  [fill A] [fill B] [fill A] [fill B] ...
GPU:            [A h2d]  [B h2d]  [A h2d]  ...
                         [A d2h]  [B d2h]  ...
```

```rust
let set = device.stream_set(2, "pipe")?;

for (i, batch) in batches.enumerate() {
    let slot   = i % 2;
    let stream = set.get(i);

    // Sync slot before reusing (waits for previous use of this slot)
    if i >= 2 { stream.synchronize()?; }

    unsafe {
        dev_bufs[slot].copy_from_host_async(batch, stream)?;
        dev_bufs[slot].copy_to_host_async(&mut results[slot], stream)?;
    }
}
set.synchronize_all()?;
```

---

## API Reference

### GpuDevice

```rust
impl GpuDevice {
    pub fn new(device_id: usize) -> Result<Self, GpuError>;
    pub fn with_backend(device_id: usize, kind: BackendKind) -> Result<Self, GpuError>;
    pub fn backend_kind(&self) -> BackendKind;
    pub fn name(&self) -> &str;
    pub fn synchronize(&self) -> Result<(), GpuError>;
    pub fn memory_info(&self) -> Result<(usize, usize), GpuError>;
    pub fn create_stream(&self, label: impl Into<String>) -> Result<GpuStream, GpuError>;
    pub fn stream_pool(&self, count: usize) -> Result<StreamPool, GpuError>;
    pub fn stream_pool_with(&self, count: usize, strategy: StreamAssignment) -> Result<StreamPool, GpuError>;
    pub fn stream_set(&self, depth: usize, label: &str) -> Result<StreamSet, GpuError>;
}
```

### GpuBuffer\<T\>

```rust
impl<T: Copy> GpuBuffer<T> {
    pub fn allocate(device: &GpuDevice, len: usize) -> Result<Self, GpuError>;
    pub fn len(&self) -> usize;
    pub fn size_bytes(&self) -> usize;
    // Sync transfers
    pub fn copy_from_host(&mut self, src: &[T]) -> Result<(), GpuError>;
    pub fn copy_to_host(&self, dst: &mut [T]) -> Result<(), GpuError>;
    // Async transfers (unsafe: caller guarantees slice lifetime)
    pub unsafe fn copy_from_host_async(&mut self, src: &[T], stream: &GpuStream) -> Result<(), GpuError>;
    pub unsafe fn copy_to_host_async(&self, dst: &mut [T], stream: &GpuStream) -> Result<(), GpuError>;
    // Tokio async (safe: takes ownership)
    pub async fn copy_from_host_async_owned(self, src: Vec<T>) -> Result<(Self, Vec<T>), GpuError>;
    pub async fn copy_to_host_async_owned(self, dst: Vec<T>) -> Result<(Self, Vec<T>), GpuError>;
}
```

### StreamPool

```rust
impl StreamPool {
    pub fn new(backend: &Arc<dyn ComputeBackend>, count: usize, strategy: StreamAssignment) -> Result<Self, GpuError>;
    pub fn acquire(&self) -> Result<StreamGuard<'_>, GpuError>;
    pub fn synchronize_all(&self) -> Result<(), GpuError>;
    pub fn len(&self) -> usize;
    pub fn iter(&self) -> impl Iterator<Item = &GpuStream>;
}
```

### GpuStream

```rust
impl GpuStream {
    pub fn new(backend: &Arc<dyn ComputeBackend>, label: impl Into<String>) -> Result<Self, GpuError>;
    pub fn id(&self) -> u64;
    pub fn label(&self) -> &str;
    pub fn synchronize(&self) -> Result<(), GpuError>;
}
```

---

## Examples

```bash
# Run the async+multi-stream demo (stub backend — no GPU required)
cargo run --example gpu_async_streams

# With CUDA
cargo run --features gpu --example gpu_async_streams

# With OpenCL
cargo run --features opencl --example gpu_async_streams

# With ROCm
ROCM_PATH=/opt/rocm cargo run --features rocm --example gpu_async_streams
```
