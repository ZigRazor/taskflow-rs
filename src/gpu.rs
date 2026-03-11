// ============================================================================
// gpu.rs — User-facing GPU API  (updated for async + multi-stream)
// ============================================================================
//
// Public surface area:
//   GpuDevice    – backend-agnostic device handle
//   GpuBuffer<T> – typed device buffer with sync + async transfer methods
//   GpuStream    – re-exported from gpu_stream
//   StreamPool   – re-exported from gpu_stream
//   GpuTaskConfig – kernel launch dimensions
//   AsyncTransfer – future-based transfer builder
// ============================================================================

pub use crate::gpu_stream::{
    GpuStream, StreamAssignment, StreamGuard, StreamPool, StreamSet,
};
pub use crate::gpu_backend::{BackendKind, GpuError};

use crate::gpu_backend::{ComputeBackend, DeviceBuffer, probe_backend};
use std::sync::Arc;
use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// GpuDevice
// ---------------------------------------------------------------------------

/// Handle to a compute device.  Wraps a pluggable `ComputeBackend`.
///
/// # Examples
/// ```ignore
/// // Auto-select best available backend (CUDA → ROCm → OpenCL → Stub)
/// let device = GpuDevice::new(0)?;
///
/// // Explicitly request OpenCL
/// let device = GpuDevice::with_backend(0, BackendKind::OpenCL)?;
/// ```
#[derive(Clone, Debug)]
pub struct GpuDevice {
    pub(crate) backend: Arc<dyn ComputeBackend>,
}

impl GpuDevice {
    /// Create a device using the best available backend.
    pub fn new(device_id: usize) -> Result<Self, GpuError> {
        let backend = probe_backend(device_id, None)?;
        Ok(Self { backend })
    }

    /// Create a device forcing a specific backend.
    pub fn with_backend(device_id: usize, kind: BackendKind) -> Result<Self, GpuError> {
        let backend = probe_backend(device_id, Some(kind))?;
        Ok(Self { backend })
    }

    /// Which backend is active.
    pub fn backend_kind(&self) -> BackendKind { self.backend.kind() }

    /// Human-readable device name.
    pub fn name(&self) -> &str { self.backend.name() }

    /// Block until all device work completes.
    pub fn synchronize(&self) -> Result<(), GpuError> {
        self.backend.synchronize_device()
    }

    /// Free / total memory in bytes.
    pub fn memory_info(&self) -> Result<(usize, usize), GpuError> {
        self.backend.memory_info()
    }

    // ---- Stream factory ---------------------------------------------------

    /// Create a single named stream.
    pub fn create_stream(&self, label: impl Into<String>) -> Result<GpuStream, GpuError> {
        GpuStream::new(&self.backend, label)
    }

    /// Create a `StreamPool` with `count` streams and round-robin assignment.
    pub fn stream_pool(&self, count: usize) -> Result<StreamPool, GpuError> {
        StreamPool::new(&self.backend, count, StreamAssignment::RoundRobin)
    }

    /// Create a `StreamPool` with a specific assignment strategy.
    pub fn stream_pool_with(
        &self,
        count: usize,
        strategy: StreamAssignment,
    ) -> Result<StreamPool, GpuError> {
        StreamPool::new(&self.backend, count, strategy)
    }

    /// Create a `StreamSet` for pipelined processing.
    pub fn stream_set(&self, depth: usize, label: &str) -> Result<StreamSet, GpuError> {
        StreamSet::new(&self.backend, depth, label)
    }
}

// ---------------------------------------------------------------------------
// GpuBuffer<T>
// ---------------------------------------------------------------------------

/// Typed device buffer.
///
/// Wraps a backend `DeviceBuffer` (opaque allocation) and adds type safety.
/// All transfers go through the active backend so the same code works with
/// CUDA, OpenCL, and ROCm.
pub struct GpuBuffer<T: Copy + 'static> {
    inner:   DeviceBuffer,
    len:     usize,
    device:  GpuDevice,
    _marker: PhantomData<T>,
}

impl<T: Copy + 'static> GpuBuffer<T> {
    // ---- Allocation -------------------------------------------------------

    /// Allocate `len` elements of type `T` on `device`, zero-initialised.
    pub fn allocate(device: &GpuDevice, len: usize) -> Result<Self, GpuError> {
        let size_bytes = len * std::mem::size_of::<T>();
        let inner = device.backend.alloc_bytes(size_bytes)?;
        Ok(Self {
            inner,
            len,
            device: device.clone(),
            _marker: PhantomData,
        })
    }

    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn size_bytes(&self) -> usize { self.len * std::mem::size_of::<T>() }

    // ---- Synchronous transfers -------------------------------------------

    /// Blocking host→device copy.
    pub fn copy_from_host(&mut self, src: &[T]) -> Result<(), GpuError> {
        assert_eq!(src.len(), self.len, "GpuBuffer::copy_from_host length mismatch");
        let bytes = src.len() * std::mem::size_of::<T>();
        self.device.backend.htod_sync(
            src.as_ptr() as *const _,
            bytes,
            &self.inner,
        )
    }

    /// Blocking device→host copy.
    pub fn copy_to_host(&self, dst: &mut [T]) -> Result<(), GpuError> {
        assert_eq!(dst.len(), self.len, "GpuBuffer::copy_to_host length mismatch");
        let bytes = dst.len() * std::mem::size_of::<T>();
        self.device.backend.dtoh_sync(
            &self.inner,
            dst.as_mut_ptr() as *mut _,
            bytes,
        )
    }

    // ---- Asynchronous transfers ------------------------------------------

    /// Enqueue a host→device copy on `stream`.
    ///
    /// Returns immediately.  The copy completes when `stream.synchronize()`
    /// returns (or when a later operation that depends on this data is synced).
    ///
    /// # Safety
    /// `src` must outlive the stream synchronisation point.  Use the safe
    /// wrapper `copy_from_host_async_owned` when lifetime guarantees are
    /// difficult to maintain.
    pub unsafe fn copy_from_host_async(
        &mut self,
        src: &[T],
        stream: &GpuStream,
    ) -> Result<(), GpuError> {
        assert_eq!(src.len(), self.len, "async H2D length mismatch");
        let bytes = src.len() * std::mem::size_of::<T>();
        self.device.backend.htod_async(
            src.as_ptr() as *const _,
            bytes,
            &self.inner,
            &stream.inner,
        )
    }

    /// Enqueue a device→host copy on `stream`.
    ///
    /// # Safety
    /// `dst` must outlive the stream synchronisation point.
    pub unsafe fn copy_to_host_async(
        &self,
        dst: &mut [T],
        stream: &GpuStream,
    ) -> Result<(), GpuError> {
        assert_eq!(dst.len(), self.len, "async D2H length mismatch");
        let bytes = dst.len() * std::mem::size_of::<T>();
        self.device.backend.dtoh_async(
            &self.inner,
            dst.as_mut_ptr() as *mut _,
            bytes,
            &stream.inner,
        )
    }

    // ---- Tokio-based async wrappers (safe) --------------------------------

    /// Safe async host→device transfer using `tokio::task::spawn_blocking`.
    ///
    /// Takes *ownership* of `src` to guarantee its lifetime while the
    /// background thread runs.  Returns `src` on completion.
    ///
    /// ```ignore
    /// let (mut buf, src) = buf.copy_from_host_async_owned(src).await?;
    /// ```
    #[cfg(feature = "async")]
    pub async fn copy_from_host_async_owned(
        mut self,
        src: Vec<T>,
    ) -> Result<(Self, Vec<T>), GpuError>
    where
        T: Send + 'static,
    {
        use tokio::task;

        // Clone what we need to move into the blocking thread
        let backend = Arc::clone(&self.device.backend);
        let inner   = Arc::clone(&self.inner);
        let len     = self.len;

        let src = task::spawn_blocking(move || -> Result<Vec<T>, GpuError> {
            assert_eq!(src.len(), len, "async_owned H2D length mismatch");
            let bytes = src.len() * std::mem::size_of::<T>();
            backend.htod_sync(src.as_ptr() as *const _, bytes, &inner)?;
            Ok(src)
        })
        .await
        .map_err(|e| GpuError::new(backend.kind(), format!("join error: {}", e)))??;

        Ok((self, src))
    }

    /// Safe async device→host transfer using `tokio::task::spawn_blocking`.
    #[cfg(feature = "async")]
    pub async fn copy_to_host_async_owned(
        self,
        mut dst: Vec<T>,
    ) -> Result<(Self, Vec<T>), GpuError>
    where
        T: Send + 'static,
    {
        use tokio::task;

        let backend = Arc::clone(&self.device.backend);
        let inner   = Arc::clone(&self.inner);
        let len     = self.len;

        let dst = task::spawn_blocking(move || -> Result<Vec<T>, GpuError> {
            assert_eq!(dst.len(), len, "async_owned D2H length mismatch");
            let bytes = dst.len() * std::mem::size_of::<T>();
            backend.dtoh_sync(&inner, dst.as_mut_ptr() as *mut _, bytes)?;
            Ok(dst)
        })
        .await
        .map_err(|e| GpuError::new(backend.kind(), format!("join error: {}", e)))??;

        Ok((self, dst))
    }

    // ---- Raw access -------------------------------------------------------

    pub fn device_buffer(&self) -> &DeviceBuffer { &self.inner }
    pub fn device(&self) -> &GpuDevice { &self.device }
}

impl<T: Copy + 'static> std::fmt::Debug for GpuBuffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GpuBuffer<{}>[len={}, backend={:?}]",
            std::any::type_name::<T>(),
            self.len,
            self.device.backend_kind(),
        )
    }
}

// ---------------------------------------------------------------------------
// GpuTaskConfig — kernel launch dimensions (unchanged)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct GpuTaskConfig {
    pub grid_dim:         (u32, u32, u32),
    pub block_dim:        (u32, u32, u32),
    pub shared_mem_bytes: u32,
}

impl Default for GpuTaskConfig {
    fn default() -> Self {
        Self { grid_dim: (1, 1, 1), block_dim: (256, 1, 1), shared_mem_bytes: 0 }
    }
}

impl GpuTaskConfig {
    pub fn linear(num_elements: usize, threads_per_block: u32) -> Self {
        let blocks = (num_elements as u32 + threads_per_block - 1) / threads_per_block;
        Self { grid_dim: (blocks, 1, 1), block_dim: (threads_per_block, 1, 1), shared_mem_bytes: 0 }
    }

    pub fn grid_2d(width: u32, height: u32, block_size: (u32, u32)) -> Self {
        let bx = (width  + block_size.0 - 1) / block_size.0;
        let by = (height + block_size.1 - 1) / block_size.1;
        Self { grid_dim: (bx, by, 1), block_dim: (block_size.0, block_size.1, 1), shared_mem_bytes: 0 }
    }

    /// Convert to cudarc's `LaunchConfig` (CUDA only).
    #[cfg(feature = "gpu")]
    pub fn to_launch_config(&self) -> cudarc::driver::LaunchConfig {
        cudarc::driver::LaunchConfig {
            grid_dim:         self.grid_dim,
            block_dim:        self.block_dim,
            shared_mem_bytes: self.shared_mem_bytes,
        }
    }
}

// ---------------------------------------------------------------------------
// AsyncTransfer builder — fluent API for complex transfer pipelines
// ---------------------------------------------------------------------------

/// Builder for constructing multi-buffer, multi-stream transfer pipelines.
///
/// # Example
/// ```ignore
/// AsyncTransfer::new(&pool)
///     .h2d(&mut buf_a, &data_a)
///     .h2d(&mut buf_b, &data_b)
///     .submit()   // enqueues all on the next stream in the pool
///     .await
/// ```
pub struct AsyncTransferBuilder<'a> {
    pool:   &'a StreamPool,
    ops:    Vec<TransferOp>,
}

enum TransferOp {
    Enqueue {
        exec: Box<dyn FnOnce(&GpuStream) -> Result<(), GpuError> + Send>,
    },
}

impl<'a> AsyncTransferBuilder<'a> {
    pub fn new(pool: &'a StreamPool) -> Self {
        Self { pool, ops: Vec::new() }
    }

    /// Enqueue a blocking H2D copy (will be deferred to submit time).
    pub fn h2d<T: Copy + 'static + Send>(
        mut self,
        buf: &'a mut GpuBuffer<T>,
        src: &'a [T],
    ) -> Self {
        // We need owned data; caller should use `h2d_owned` for safe async.
        let backend = Arc::clone(&buf.device.backend);
        let inner   = Arc::clone(&buf.inner);
        let ptr     = src.as_ptr() as usize; // transmit pointer as usize
        let bytes   = src.len() * std::mem::size_of::<T>();

        self.ops.push(TransferOp::Enqueue {
            exec: Box::new(move |_stream| {
                backend.htod_sync(ptr as *const _, bytes, &inner)
            }),
        });
        self
    }

    /// Execute all queued operations, each on a stream from the pool.
    ///
    /// Returns once all operations have been *enqueued* (not necessarily
    /// completed).  Call `pool.synchronize_all()` for completion.
    pub fn submit(self) -> Result<(), GpuError> {
        for op in self.ops {
            let guard = self.pool.acquire()?;
            match op {
                TransferOp::Enqueue { exec } => {
                    exec(guard.stream())?;
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_backend::BackendKind;

    fn stub_device() -> GpuDevice {
        GpuDevice::with_backend(0, BackendKind::Stub).expect("stub backend")
    }

    #[test]
    fn test_buffer_allocate() {
        let dev = stub_device();
        let buf: GpuBuffer<f32> = GpuBuffer::allocate(&dev, 1024).unwrap();
        assert_eq!(buf.len(), 1024);
        assert_eq!(buf.size_bytes(), 1024 * 4);
    }

    #[test]
    fn test_sync_transfer_roundtrip() {
        let dev = stub_device();
        let mut buf: GpuBuffer<f32> = GpuBuffer::allocate(&dev, 4).unwrap();
        let src = vec![1.0f32, 2.0, 3.0, 4.0];
        buf.copy_from_host(&src).unwrap();

        let mut dst = vec![0.0f32; 4];
        buf.copy_to_host(&mut dst).unwrap();
        // Stub backend is a no-op, so dst stays 0 — we verify no panic/error
    }

    #[test]
    fn test_device_stream_pool() {
        let dev = stub_device();
        let pool = dev.stream_pool(4).unwrap();
        assert_eq!(pool.len(), 4);
        pool.synchronize_all().unwrap();
    }

    #[test]
    fn test_stream_set_pipeline() {
        let dev = stub_device();
        let set = dev.stream_set(3, "compute").unwrap();
        for i in 0..9 {
            let s = set.get(i);
            s.synchronize().unwrap();
        }
    }

    #[test]
    fn test_task_config_linear() {
        let cfg = GpuTaskConfig::linear(1024, 256);
        assert_eq!(cfg.grid_dim, (4, 1, 1));
        assert_eq!(cfg.block_dim, (256, 1, 1));
    }

    #[test]
    fn test_task_config_2d() {
        let cfg = GpuTaskConfig::grid_2d(1920, 1080, (16, 16));
        assert_eq!(cfg.grid_dim, (120, 68, 1));
        assert_eq!(cfg.block_dim, (16, 16, 1));
    }

    #[test]
    fn test_memory_info() {
        let dev = stub_device();
        let (free, total) = dev.memory_info().unwrap();
        assert!(free <= total);
        assert!(total > 0);
    }
}
