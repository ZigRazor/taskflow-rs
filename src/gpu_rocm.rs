// ============================================================================
// gpu_rocm.rs — ROCm / AMD HIP Backend
// ============================================================================
//
// Implements `ComputeBackend` using the AMD HIP runtime library directly via
// FFI.  This avoids a dependency on an immature Rust HIP crate while still
// providing real ROCm support.
//
// The HIP API intentionally mirrors CUDA so most of the code maps 1-to-1.
//
// Compiled only when `--features rocm` is active.
//
// Prerequisites:
//   - ROCm ≥ 5.0 installed (/opt/rocm)
//   - `amdhip64` shared library available
//   - Build with: ROCM_PATH=/opt/rocm cargo build --features rocm
// ============================================================================

#![cfg(feature = "rocm")]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::gpu_backend::{
    BackendBuffer, BackendKind, BackendStream, ComputeBackend,
    DeviceBuffer, DeviceStream, GpuError,
};

// ---------------------------------------------------------------------------
// HIP FFI bindings
// ---------------------------------------------------------------------------

/// HIP device pointer type (equivalent to CUdeviceptr)
pub type HipDevicePtr = u64;

/// HIP stream handle (opaque pointer)
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct HipStream(pub *mut std::ffi::c_void);

unsafe impl Send for HipStream {}
unsafe impl Sync for HipStream {}

impl HipStream {
    pub fn null() -> Self { HipStream(std::ptr::null_mut()) }
}

/// HIP error code (hipError_t)
pub type HipError = i32;
pub const HIP_SUCCESS: HipError = 0;

#[link(name = "amdhip64", kind = "dylib")]
extern "C" {
    fn hipSetDevice(device_id: i32) -> HipError;
    fn hipGetDeviceCount(count: *mut i32) -> HipError;
    fn hipDeviceGetName(name: *mut u8, len: i32, device: i32) -> HipError;
    fn hipMalloc(ptr: *mut *mut c_void, size: usize) -> HipError;
    fn hipFree(ptr: *mut c_void) -> HipError;
    fn hipMemcpy(dst: *mut c_void, src: *const c_void, size: usize, kind: i32) -> HipError;
    fn hipMemcpyAsync(
        dst: *mut c_void,
        src: *const c_void,
        size: usize,
        kind: i32,
        stream: HipStream,
    ) -> HipError;
    fn hipStreamCreate(stream: *mut HipStream) -> HipError;
    fn hipStreamDestroy(stream: HipStream) -> HipError;
    fn hipStreamSynchronize(stream: HipStream) -> HipError;
    fn hipDeviceSynchronize() -> HipError;
    fn hipMemGetInfo(free: *mut usize, total: *mut usize) -> HipError;
}

// hipMemcpyKind constants
const HIP_MEMCPY_HOST_TO_DEVICE: i32 = 1;
const HIP_MEMCPY_DEVICE_TO_HOST: i32 = 2;

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn rocm_err(code: HipError, op: &str) -> GpuError {
    GpuError::rocm(format!("{} failed with hipError_t = {}", op, code))
}

fn check(code: HipError, op: &str) -> Result<(), GpuError> {
    if code == HIP_SUCCESS { Ok(()) } else { Err(rocm_err(code, op)) }
}

// ---------------------------------------------------------------------------
// RocmBuffer — owns a HIP device allocation
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RocmBuffer {
    ptr:        *mut c_void,
    size_bytes: usize,
}

// SAFETY: HIP device pointers may be used from any thread.
unsafe impl Send for RocmBuffer {}
unsafe impl Sync for RocmBuffer {}

impl RocmBuffer {
    fn allocate(size: usize) -> Result<Self, GpuError> {
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let code = unsafe { hipMalloc(&mut ptr as *mut *mut _, size) };
        check(code, "hipMalloc")?;
        Ok(Self { ptr, size_bytes: size })
    }
}

impl Drop for RocmBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { hipFree(self.ptr); }
        }
    }
}

impl BackendBuffer for RocmBuffer {
    fn size_bytes(&self) -> usize { self.size_bytes }
    fn device_ptr(&self) -> *const c_void { self.ptr }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ---------------------------------------------------------------------------
// RocmStream — wraps a HIP stream
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RocmStream {
    pub(crate) stream: HipStream,
    pub(crate) id:     u64,
}

unsafe impl Send for RocmStream {}
unsafe impl Sync for RocmStream {}

impl RocmStream {
    fn create(id: u64) -> Result<Self, GpuError> {
        let mut stream = HipStream::null();
        let code = unsafe { hipStreamCreate(&mut stream) };
        check(code, "hipStreamCreate")?;
        Ok(Self { stream, id })
    }
}

impl Drop for RocmStream {
    fn drop(&mut self) {
        if !self.stream.0.is_null() {
            unsafe { hipStreamDestroy(self.stream); }
        }
    }
}

impl BackendStream for RocmStream {
    fn id(&self) -> u64 { self.id }

    fn synchronize(&self) -> Result<(), GpuError> {
        let code = unsafe { hipStreamSynchronize(self.stream) };
        check(code, "hipStreamSynchronize")
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ---------------------------------------------------------------------------
// RocmBackend
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RocmBackend {
    device_id:      usize,
    name:           String,
    next_stream_id: AtomicU64,
}

impl RocmBackend {
    pub fn new(device_id: usize) -> Result<Self, GpuError> {
        // Verify device count
        let mut count: i32 = 0;
        let code = unsafe { hipGetDeviceCount(&mut count) };
        check(code, "hipGetDeviceCount")?;

        if device_id >= count as usize {
            return Err(GpuError::rocm(format!(
                "ROCm device_id {} out of range ({} devices)", device_id, count
            )));
        }

        // Set active device for this thread
        let code = unsafe { hipSetDevice(device_id as i32) };
        check(code, "hipSetDevice")?;

        // Query device name
        let mut name_buf = vec![0u8; 256];
        let code = unsafe {
            hipDeviceGetName(name_buf.as_mut_ptr(), 256, device_id as i32)
        };
        check(code, "hipDeviceGetName")?;
        let name = String::from_utf8_lossy(
            &name_buf[..name_buf.iter().position(|&b| b == 0).unwrap_or(256)]
        ).into_owned();

        log::info!("ROCm backend initialised: {} (device {})", name, device_id);

        Ok(Self {
            device_id,
            name,
            next_stream_id: AtomicU64::new(1),
        })
    }

    fn set_device(&self) -> Result<(), GpuError> {
        let code = unsafe { hipSetDevice(self.device_id as i32) };
        check(code, "hipSetDevice")
    }
}

impl ComputeBackend for RocmBackend {
    fn kind(&self)      -> BackendKind { BackendKind::Rocm }
    fn name(&self)      -> &str        { &self.name }
    fn device_id(&self) -> usize       { self.device_id }

    // ---- Memory -----------------------------------------------------------

    fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError> {
        self.set_device()?;
        let buf = RocmBuffer::allocate(size)?;
        Ok(Arc::new(buf))
    }

    // ---- Synchronous transfers -------------------------------------------

    fn htod_sync(
        &self,
        src: *const c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
    ) -> Result<(), GpuError> {
        self.set_device()?;
        let dst_buf = dst_as_rocm(dst)?;
        let code = unsafe {
            hipMemcpy(dst_buf.ptr, src, src_bytes, HIP_MEMCPY_HOST_TO_DEVICE)
        };
        check(code, "hipMemcpy(H2D)")
    }

    fn dtoh_sync(
        &self,
        src: &DeviceBuffer,
        dst: *mut c_void,
        dst_bytes: usize,
    ) -> Result<(), GpuError> {
        self.set_device()?;
        let src_buf = dst_as_rocm(src)?;
        let code = unsafe {
            hipMemcpy(dst, src_buf.ptr, dst_bytes, HIP_MEMCPY_DEVICE_TO_HOST)
        };
        check(code, "hipMemcpy(D2H)")
    }

    // ---- Asynchronous transfers ------------------------------------------

    unsafe fn htod_async(
        &self,
        src: *const c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
        stream: &DeviceStream,
    ) -> Result<(), GpuError> {
        let dst_buf     = dst_as_rocm(dst)?;
        let hip_stream  = stream_as_rocm(stream)?;

        let code = hipMemcpyAsync(
            dst_buf.ptr,
            src,
            src_bytes,
            HIP_MEMCPY_HOST_TO_DEVICE,
            hip_stream,
        );
        check(code, "hipMemcpyAsync(H2D)")
    }

    unsafe fn dtoh_async(
        &self,
        src: &DeviceBuffer,
        dst: *mut c_void,
        dst_bytes: usize,
        stream: &DeviceStream,
    ) -> Result<(), GpuError> {
        let src_buf     = dst_as_rocm(src)?;
        let hip_stream  = stream_as_rocm(stream)?;

        let code = hipMemcpyAsync(
            dst,
            src_buf.ptr,
            dst_bytes,
            HIP_MEMCPY_DEVICE_TO_HOST,
            hip_stream,
        );
        check(code, "hipMemcpyAsync(D2H)")
    }

    // ---- Streams ---------------------------------------------------------

    fn create_stream(&self) -> Result<DeviceStream, GpuError> {
        self.set_device()?;
        let id     = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
        let stream = RocmStream::create(id)?;
        Ok(Arc::new(stream))
    }

    // ---- Device sync -----------------------------------------------------

    fn synchronize_device(&self) -> Result<(), GpuError> {
        self.set_device()?;
        let code = unsafe { hipDeviceSynchronize() };
        check(code, "hipDeviceSynchronize")
    }

    // ---- Memory info -----------------------------------------------------

    fn memory_info(&self) -> Result<(usize, usize), GpuError> {
        self.set_device()?;
        let mut free:  usize = 0;
        let mut total: usize = 0;
        let code = unsafe { hipMemGetInfo(&mut free, &mut total) };
        check(code, "hipMemGetInfo")?;
        Ok((free, total))
    }
}

// ---------------------------------------------------------------------------
// Downcast helpers
// ---------------------------------------------------------------------------

fn dst_as_rocm(buf: &DeviceBuffer) -> Result<&RocmBuffer, GpuError> {
    buf.as_any()
        .downcast_ref::<RocmBuffer>()
        .ok_or_else(|| GpuError::rocm("DeviceBuffer is not a ROCm buffer"))
}

fn stream_as_rocm(stream: &DeviceStream) -> Result<HipStream, GpuError> {
    stream.as_any()
        .downcast_ref::<RocmStream>()
        .map(|s| s.stream)
        .ok_or_else(|| GpuError::rocm("DeviceStream is not a ROCm stream"))
}
