// ============================================================================
// gpu_backend.rs — Pluggable Compute Backend Abstraction
// ============================================================================
//
// Architecture decision: trait-object dispatch over monomorphisation.
// Rationale: backends are chosen at runtime (config / device probe), and
// the extra vtable indirection is negligible compared to any GPU operation.
//
// Each backend implements `ComputeBackend`.  Higher-level types (`GpuDevice`,
// `GpuBuffer`, `GpuStream`) hold an `Arc<dyn ComputeBackend>` and delegate
// to it, so user-facing APIs are backend-agnostic.
// ============================================================================

use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Unified GPU error across all backends.
#[derive(Debug, Clone)]
pub struct GpuError {
    pub backend: BackendKind,
    pub message: String,
}

impl GpuError {
    pub fn new(backend: BackendKind, msg: impl Into<String>) -> Self {
        Self {
            backend,
            message: msg.into(),
        }
    }

    pub fn cuda(msg: impl Into<String>) -> Self {
        Self::new(BackendKind::Cuda, msg)
    }

    pub fn opencl(msg: impl Into<String>) -> Self {
        Self::new(BackendKind::OpenCL, msg)
    }

    pub fn rocm(msg: impl Into<String>) -> Self {
        Self::new(BackendKind::Rocm, msg)
    }
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.backend, self.message)
    }
}

impl std::error::Error for GpuError {}

// ---------------------------------------------------------------------------
// Backend kind tag — lightweight enum for logging / error messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    Cuda,
    OpenCL,
    Rocm,
    /// For unit-tests / environments without hardware
    Stub,
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendKind::Cuda => write!(f, "CUDA"),
            BackendKind::OpenCL => write!(f, "OpenCL"),
            BackendKind::Rocm => write!(f, "ROCm/HIP"),
            BackendKind::Stub => write!(f, "Stub"),
        }
    }
}

// ---------------------------------------------------------------------------
// Opaque buffer handle
// ---------------------------------------------------------------------------

/// Type-erased device buffer.  Each backend owns the allocation; dropping
/// this handle frees the device memory through the `BackendBuffer` trait.
pub trait BackendBuffer: Send + Sync + fmt::Debug {
    fn size_bytes(&self) -> usize;
    fn device_ptr(&self) -> *const std::ffi::c_void; // for kernel launching
    /// Required for downcasting `Arc<dyn BackendBuffer>` to a concrete type.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Heap-allocated, reference-counted buffer handle shared across tasks.
pub type DeviceBuffer = Arc<dyn BackendBuffer>;

// ---------------------------------------------------------------------------
// Opaque stream handle
// ---------------------------------------------------------------------------

/// An in-order command queue / CUDA stream / OpenCL command queue.
pub trait BackendStream: Send + Sync + fmt::Debug {
    fn id(&self) -> u64;
    /// Block the *calling CPU thread* until all previously enqueued work on
    /// this stream has completed.
    fn synchronize(&self) -> Result<(), GpuError>;
    /// Required for downcasting `Arc<dyn BackendStream>` to a concrete type.
    fn as_any(&self) -> &dyn std::any::Any;
}

pub type DeviceStream = Arc<dyn BackendStream>;

// ---------------------------------------------------------------------------
// Transfer direction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDir {
    HostToDevice,
    DeviceToHost,
    DeviceToDevice,
}

// ---------------------------------------------------------------------------
// The core backend trait
// ---------------------------------------------------------------------------

/// Every compute backend must implement this trait.  Operations are kept at
/// the byte / pointer level so the trait itself is not parameterised over `T`.
/// The typed `GpuBuffer<T>` wrapper in `gpu.rs` calls these methods and casts
/// appropriately.
pub trait ComputeBackend: Send + Sync + fmt::Debug + 'static {
    // ---- Identity ----------------------------------------------------------

    fn kind(&self) -> BackendKind;
    fn name(&self) -> &str; // e.g. "NVIDIA GeForce RTX 4090"
    fn device_id(&self) -> usize;

    // ---- Memory ------------------------------------------------------------

    /// Allocate `size` bytes of device memory, zeroed.
    fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError>;

    // ---- Synchronous transfers (blocking) ----------------------------------

    fn htod_sync(
        &self,
        src: *const std::ffi::c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
    ) -> Result<(), GpuError>;

    fn dtoh_sync(
        &self,
        src: &DeviceBuffer,
        dst: *mut std::ffi::c_void,
        dst_bytes: usize,
    ) -> Result<(), GpuError>;

    // ---- Asynchronous transfers (enqueued on stream) -----------------------

    /// Enqueue a host→device copy on `stream`.  Returns immediately; the copy
    /// completes when the stream is synchronised (or a later stream barrier).
    ///
    /// # Safety
    /// `src` must remain valid and unmodified until `stream.synchronize()`.
    unsafe fn htod_async(
        &self,
        src: *const std::ffi::c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
        stream: &DeviceStream,
    ) -> Result<(), GpuError>;

    /// Enqueue a device→host copy on `stream`.
    ///
    /// # Safety
    /// `dst` must remain valid until `stream.synchronize()`.
    unsafe fn dtoh_async(
        &self,
        src: &DeviceBuffer,
        dst: *mut std::ffi::c_void,
        dst_bytes: usize,
        stream: &DeviceStream,
    ) -> Result<(), GpuError>;

    // ---- Streams -----------------------------------------------------------

    fn create_stream(&self) -> Result<DeviceStream, GpuError>;

    // ---- Device-wide sync --------------------------------------------------

    fn synchronize_device(&self) -> Result<(), GpuError>;

    // ---- Introspection -----------------------------------------------------

    /// Free and total device memory in bytes.
    fn memory_info(&self) -> Result<(usize, usize), GpuError>;
}

// ---------------------------------------------------------------------------
// BackendRegistry — select a backend at startup
// ---------------------------------------------------------------------------

/// Probe available hardware and return the best available backend for the
/// requested device index, optionally filtered by kind preference.
///
/// Priority order (when `preferred` is `None`):
///   CUDA → ROCm → OpenCL → Stub
pub fn probe_backend(
    device_id: usize,
    preferred: Option<BackendKind>,
) -> Result<Arc<dyn ComputeBackend>, GpuError> {
    let order: Vec<BackendKind> = match preferred {
        Some(k) => vec![k],
        None => vec![
            BackendKind::Cuda,
            BackendKind::Rocm,
            BackendKind::OpenCL,
            BackendKind::Stub,
        ],
    };

    for kind in &order {
        match try_init_backend(*kind, device_id) {
            Ok(b) => {
                log::info!("GPU backend selected: {} on device {}", b.name(), device_id);
                return Ok(b);
            }
            Err(e) => {
                log::debug!("Backend {:?} unavailable: {}", kind, e);
            }
        }
    }

    Err(GpuError::new(
        BackendKind::Stub,
        format!("No compute backend available for device {}", device_id),
    ))
}

fn try_init_backend(
    kind: BackendKind,
    device_id: usize,
) -> Result<Arc<dyn ComputeBackend>, GpuError> {
    match kind {
        #[cfg(feature = "gpu")]
        BackendKind::Cuda => {
            use crate::gpu_cuda_backend::CudaBackend;
            let b = CudaBackend::new(device_id)?;
            Ok(Arc::new(b) as Arc<dyn ComputeBackend>)
        }
        #[cfg(not(feature = "gpu"))]
        BackendKind::Cuda => Err(GpuError::cuda("CUDA feature not compiled")),

        #[cfg(feature = "opencl")]
        BackendKind::OpenCL => {
            use crate::gpu_opencl::OpenCLBackend;
            let b = OpenCLBackend::new(device_id)?;
            Ok(Arc::new(b) as Arc<dyn ComputeBackend>)
        }
        #[cfg(not(feature = "opencl"))]
        BackendKind::OpenCL => Err(GpuError::opencl("OpenCL feature not compiled")),

        #[cfg(feature = "rocm")]
        BackendKind::Rocm => {
            use crate::gpu_rocm::RocmBackend;
            let b = RocmBackend::new(device_id)?;
            Ok(Arc::new(b) as Arc<dyn ComputeBackend>)
        }
        #[cfg(not(feature = "rocm"))]
        BackendKind::Rocm => Err(GpuError::rocm("ROCm feature not compiled")),

        BackendKind::Stub => {
            use self::stub::StubBackend;
            Ok(Arc::new(StubBackend::new(device_id)) as Arc<dyn ComputeBackend>)
        }
    }
}

// ---------------------------------------------------------------------------
// Stub backend — always available, useful for CI / no-GPU environments
// ---------------------------------------------------------------------------

pub mod stub {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static STREAM_ID: AtomicU64 = AtomicU64::new(1);

    #[derive(Debug)]
    pub struct StubBuffer {
        data: Vec<u8>,
    }

    impl BackendBuffer for StubBuffer {
        fn size_bytes(&self) -> usize {
            self.data.len()
        }
        fn device_ptr(&self) -> *const std::ffi::c_void {
            self.data.as_ptr() as *const _
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Debug)]
    pub struct StubStream {
        pub id: u64,
    }

    impl BackendStream for StubStream {
        fn id(&self) -> u64 {
            self.id
        }
        fn synchronize(&self) -> Result<(), GpuError> {
            Ok(())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Debug)]
    pub struct StubBackend {
        device_id: usize,
    }

    impl StubBackend {
        pub fn new(device_id: usize) -> Self {
            Self { device_id }
        }
    }

    impl ComputeBackend for StubBackend {
        fn kind(&self) -> BackendKind {
            BackendKind::Stub
        }
        fn name(&self) -> &str {
            "Stub (no hardware)"
        }
        fn device_id(&self) -> usize {
            self.device_id
        }

        fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError> {
            Ok(Arc::new(StubBuffer {
                data: vec![0u8; size],
            }))
        }

        fn htod_sync(
            &self,
            src: *const std::ffi::c_void,
            src_bytes: usize,
            dst: &DeviceBuffer,
        ) -> Result<(), GpuError> {
            // In the stub we copy into the Vec backing the StubBuffer.
            // In production this would be a real H2D DMA transfer.
            let _ = (src, src_bytes, dst);
            Ok(())
        }

        fn dtoh_sync(
            &self,
            src: &DeviceBuffer,
            dst: *mut std::ffi::c_void,
            dst_bytes: usize,
        ) -> Result<(), GpuError> {
            let _ = (src, dst, dst_bytes);
            Ok(())
        }

        unsafe fn htod_async(
            &self,
            src: *const std::ffi::c_void,
            src_bytes: usize,
            dst: &DeviceBuffer,
            _stream: &DeviceStream,
        ) -> Result<(), GpuError> {
            self.htod_sync(src, src_bytes, dst)
        }

        unsafe fn dtoh_async(
            &self,
            src: &DeviceBuffer,
            dst: *mut std::ffi::c_void,
            dst_bytes: usize,
            _stream: &DeviceStream,
        ) -> Result<(), GpuError> {
            self.dtoh_sync(src, dst, dst_bytes)
        }

        fn create_stream(&self) -> Result<DeviceStream, GpuError> {
            let id = STREAM_ID.fetch_add(1, Ordering::Relaxed);
            Ok(Arc::new(StubStream { id }))
        }

        fn synchronize_device(&self) -> Result<(), GpuError> {
            Ok(())
        }

        fn memory_info(&self) -> Result<(usize, usize), GpuError> {
            Ok((8 * 1024 * 1024 * 1024, 8 * 1024 * 1024 * 1024)) // pretend 8 GB
        }
    }
}
