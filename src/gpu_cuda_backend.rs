// ============================================================================
// gpu_cuda_backend.rs — CUDA backend via cudarc 0.11
// ============================================================================

#![cfg(feature = "gpu")]

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use cudarc::driver::{
    CudaDevice, CudaSlice, CudaStream,
    DeviceSlice, DevicePtr,   // trait imports for .len() and .device_ptr()
};

use crate::gpu_backend::{
    BackendBuffer, BackendKind, BackendStream, ComputeBackend,
    DeviceBuffer, DeviceStream, GpuError,
};

// ---------------------------------------------------------------------------
// CudaDeviceBuffer
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CudaDeviceBuffer {
    // Mutex gives us the &mut CudaSlice needed by htod_sync_copy_into
    // without undefined behaviour.
    pub(crate) inner: Mutex<CudaSlice<u8>>,
}

impl BackendBuffer for CudaDeviceBuffer {
    fn size_bytes(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    fn device_ptr(&self) -> *const c_void {
        *self.inner.lock().unwrap().device_ptr() as usize as *const c_void
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ---------------------------------------------------------------------------
// CudaStreamHandle
// ---------------------------------------------------------------------------

/// `CudaStream` holds a raw `*mut CUstream_st` which the compiler cannot
/// prove is Send/Sync, but the CUDA driver guarantees stream handles are safe
/// to pass between threads.
pub struct CudaStreamHandle {
    pub(crate) stream:    CudaStream,
    pub(crate) stream_id: u64,
}

// SAFETY: CUDA stream handles are opaque driver objects safe to use from
// any CPU thread.
unsafe impl Send for CudaStreamHandle {}
unsafe impl Sync for CudaStreamHandle {}

impl std::fmt::Debug for CudaStreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CudaStreamHandle(id={})", self.stream_id)
    }
}

impl BackendStream for CudaStreamHandle {
    fn id(&self) -> u64 { self.stream_id }

    fn synchronize(&self) -> Result<(), GpuError> {
        // cudarc 0.11: per-stream sync via cudarc::driver::result::stream::synchronize,
        // which wraps cuStreamSynchronize and takes the raw CUstream field.
        // CudaStream.stream is the public CUstream handle.
        unsafe { cudarc::driver::result::stream::synchronize(self.stream.stream) }
            .map_err(|e| GpuError::cuda(format!("stream synchronize failed: {:?}", e)))
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ---------------------------------------------------------------------------
// CudaBackend
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CudaBackend {
    device:         Arc<CudaDevice>,
    device_id:      usize,
    device_name:    String,
    next_stream_id: std::sync::atomic::AtomicU64,
}

impl CudaBackend {
    pub fn new(device_id: usize) -> Result<Self, GpuError> {
        let device = CudaDevice::new(device_id)
            .map_err(|e| GpuError::cuda(format!(
                "Failed to initialise CUDA device {}: {:?}", device_id, e
            )))?;

        let device_name = format!("CUDA Device {}", device_id);

        Ok(Self {
            device,
            device_id,
            device_name,
            next_stream_id: std::sync::atomic::AtomicU64::new(1),
        })
    }
}

impl ComputeBackend for CudaBackend {
    fn kind(&self)      -> BackendKind { BackendKind::Cuda }
    fn name(&self)      -> &str        { &self.device_name }
    fn device_id(&self) -> usize       { self.device_id }

    // ---- Memory -----------------------------------------------------------

    fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError> {
        let slice: CudaSlice<u8> = self.device
            .alloc_zeros::<u8>(size)
            .map_err(|e| GpuError::cuda(format!("alloc_zeros({}) failed: {:?}", size, e)))?;

        Ok(Arc::new(CudaDeviceBuffer {
            inner: Mutex::new(slice),
        }))
    }

    // ---- Synchronous transfers -------------------------------------------

    fn htod_sync(
        &self,
        src: *const c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
    ) -> Result<(), GpuError> {
        let dst_buf = downcast_buffer(dst)?;

        let host_slice: &[u8] = unsafe {
            std::slice::from_raw_parts(src as *const u8, src_bytes)
        };

        self.device
            .htod_sync_copy_into(host_slice, &mut *dst_buf.inner.lock().unwrap())
            .map_err(|e| GpuError::cuda(format!("htod_sync failed: {:?}", e)))
    }

    fn dtoh_sync(
        &self,
        src: &DeviceBuffer,
        dst: *mut c_void,
        dst_bytes: usize,
    ) -> Result<(), GpuError> {
        let src_buf = downcast_buffer(src)?;

        let host_slice: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(dst as *mut u8, dst_bytes)
        };

        self.device
            .dtoh_sync_copy_into(&*src_buf.inner.lock().unwrap(), host_slice)
            .map_err(|e| GpuError::cuda(format!("dtoh_sync failed: {:?}", e)))
    }

    // ---- Asynchronous transfers ------------------------------------------
    //
    // cudarc 0.11 does not expose a typed stream-based async memcpy surface.
    // True DMA overlap is achieved at the GpuBuffer level via
    // `copy_from_host_async_owned` / `tokio::task::spawn_blocking`, which
    // runs the blocking copy on a thread-pool thread without blocking the
    // async runtime.  This path is the synchronous fallback used by the
    // low-level unsafe API.

    unsafe fn htod_async(
        &self,
        src: *const c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
        _stream: &DeviceStream,
    ) -> Result<(), GpuError> {
        self.htod_sync(src, src_bytes, dst)
    }

    unsafe fn dtoh_async(
        &self,
        src: &DeviceBuffer,
        dst: *mut c_void,
        dst_bytes: usize,
        _stream: &DeviceStream,
    ) -> Result<(), GpuError> {
        self.dtoh_sync(src, dst, dst_bytes)
    }

    // ---- Streams ---------------------------------------------------------

    fn create_stream(&self) -> Result<DeviceStream, GpuError> {
        let stream = self.device
            .fork_default_stream()
            .map_err(|e| GpuError::cuda(format!("fork_default_stream failed: {:?}", e)))?;

        let id = self.next_stream_id.fetch_add(
            1,
            std::sync::atomic::Ordering::Relaxed,
        );

        Ok(Arc::new(CudaStreamHandle {
            stream,
            stream_id: id,
        }))
    }

    // ---- Device sync -----------------------------------------------------

    fn synchronize_device(&self) -> Result<(), GpuError> {
        self.device
            .synchronize()
            .map_err(|e| GpuError::cuda(format!("Device synchronize failed: {:?}", e)))
    }

    // ---- Memory info -----------------------------------------------------

    fn memory_info(&self) -> Result<(usize, usize), GpuError> {
        // cudarc 0.11: cudarc::driver::result::memory_get_info wraps cuMemGetInfo_v2.
        cudarc::driver::result::mem_get_info()
            .map_err(|e| GpuError::cuda(format!("memory_get_info failed: {:?}", e)))
    }
}

// ---------------------------------------------------------------------------
// Downcast helpers
// ---------------------------------------------------------------------------

pub(crate) fn downcast_buffer(buf: &DeviceBuffer) -> Result<&CudaDeviceBuffer, GpuError> {
    buf.as_any()
        .downcast_ref::<CudaDeviceBuffer>()
        .ok_or_else(|| GpuError::cuda("DeviceBuffer is not a CudaDeviceBuffer"))
}

