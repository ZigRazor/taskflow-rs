// ============================================================================
// gpu_opencl.rs — OpenCL Backend
// ============================================================================
//
// Implements `ComputeBackend` using the `opencl3` crate.
// Compiled only when `--features opencl` is active.
//
// Works on:
//   - NVIDIA GPUs (CUDA driver exposes OpenCL 3.0)
//   - AMD GPUs    (ROCm ships an OpenCL runtime)
//   - Intel GPUs  (Intel OpenCL SDK / oneAPI)
//   - Apple CPU/GPU (via Apple's OpenCL runtime — deprecated but functional)
// ============================================================================

#![cfg(feature = "opencl")]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use opencl3::{
    command_queue::{CommandQueue, CL_QUEUE_PROFILING_ENABLE},
    context::Context,
    device::{Device, CL_DEVICE_TYPE_ALL, CL_DEVICE_TYPE_GPU},
    memory::{Buffer as ClBuffer, CL_MEM_READ_WRITE},
    platform::get_platforms,
};

use crate::gpu_backend::{
    BackendBuffer, BackendKind, BackendStream, ComputeBackend, DeviceBuffer, DeviceStream, GpuError,
};

// ---------------------------------------------------------------------------
// OpenCL error helpers
// ---------------------------------------------------------------------------

fn ocl_err(msg: impl Into<String>) -> GpuError {
    GpuError::opencl(msg)
}

// ---------------------------------------------------------------------------
// OpenCLBuffer
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct OpenCLBuffer {
    /// Mutex gives &mut ClBuffer needed by enqueue_write/read_buffer.
    pub(crate) buf: Mutex<ClBuffer<u8>>,
    pub(crate) size_bytes: usize,
}

// SAFETY: OpenCL buffers are reference-counted internally; sharing across
// threads is safe provided we don't modify them concurrently.
unsafe impl Send for OpenCLBuffer {}
unsafe impl Sync for OpenCLBuffer {}

impl BackendBuffer for OpenCLBuffer {
    fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    fn device_ptr(&self) -> *const c_void {
        // OpenCL buffers don't have a raw device pointer accessible from
        // the host. We return a null pointer; kernel code gets the buffer
        // via the cl_mem handle, not a raw pointer.
        std::ptr::null()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// OpenCLStream
// ---------------------------------------------------------------------------

/// Wraps a `CommandQueue`, which is OpenCL's equivalent of a CUDA stream:
/// an ordered sequence of operations on a device.
#[derive(Debug)]
pub struct OpenCLStream {
    pub(crate) queue: CommandQueue,
    pub(crate) id: u64,
}

unsafe impl Send for OpenCLStream {}
unsafe impl Sync for OpenCLStream {}

impl BackendStream for OpenCLStream {
    fn id(&self) -> u64 {
        self.id
    }

    fn synchronize(&self) -> Result<(), GpuError> {
        self.queue
            .finish()
            .map_err(|e| ocl_err(format!("CommandQueue::finish failed: {:?}", e)))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// OpenCLBackend
// ---------------------------------------------------------------------------

pub struct OpenCLBackend {
    pub(crate) context: Arc<Context>,
    pub(crate) device: Device,
    pub(crate) device_id: usize,
    pub(crate) name: String,
    next_stream_id: AtomicU64,
    /// Default command queue (used for synchronous operations).
    default_queue: Arc<Mutex<CommandQueue>>,
}

impl std::fmt::Debug for OpenCLBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpenCLBackend({})", self.name)
    }
}

unsafe impl Send for OpenCLBackend {}
unsafe impl Sync for OpenCLBackend {}

impl OpenCLBackend {
    /// Initialise OpenCL for the given logical device index.
    ///
    /// Iterates all platforms and all GPU devices, selecting the one at
    /// position `device_id` in that flat list.
    pub fn new(device_id: usize) -> Result<Self, GpuError> {
        // Enumerate all platforms
        let platforms =
            get_platforms().map_err(|e| ocl_err(format!("get_platforms failed: {:?}", e)))?;

        // Collect all GPU devices across all platforms
        let mut all_devices: Vec<(Device, String)> = Vec::new();

        for platform in &platforms {
            let devs = platform.get_devices(CL_DEVICE_TYPE_GPU).unwrap_or_default();

            for d in devs {
                let dev = Device::new(d);
                let name = dev
                    .name()
                    .unwrap_or_else(|_| "Unknown OpenCL Device".to_string());
                all_devices.push((dev, name));
            }
        }

        // Fall back to any device if no GPU found (CPU OpenCL, etc.)
        if all_devices.is_empty() {
            for platform in &platforms {
                let devs = platform.get_devices(CL_DEVICE_TYPE_ALL).unwrap_or_default();
                for d in devs {
                    let dev = Device::new(d);
                    let name = dev.name().unwrap_or_default();
                    all_devices.push((dev, name));
                }
            }
        }

        if all_devices.is_empty() {
            return Err(ocl_err("No OpenCL devices found on this system"));
        }

        if device_id >= all_devices.len() {
            return Err(ocl_err(format!(
                "OpenCL device_id {} out of range ({} devices available)",
                device_id,
                all_devices.len()
            )));
        }

        let (device, name) = all_devices.remove(device_id);

        let context = Context::from_device(&device)
            .map_err(|e| ocl_err(format!("Context::from_device failed: {:?}", e)))?;

        // Create a default in-order command queue
        let default_queue =
            CommandQueue::create_default_with_properties(&context, CL_QUEUE_PROFILING_ENABLE, 0)
                .map_err(|e| ocl_err(format!("CommandQueue::create_default failed: {:?}", e)))?;

        log::info!(
            "OpenCL backend initialised: {} (device {})",
            name,
            device_id
        );

        Ok(Self {
            context: Arc::new(context),
            device,
            device_id,
            name,
            next_stream_id: AtomicU64::new(1),
            default_queue: Arc::new(Mutex::new(default_queue)),
        })
    }
}

impl ComputeBackend for OpenCLBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::OpenCL
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn device_id(&self) -> usize {
        self.device_id
    }

    // ---- Memory -----------------------------------------------------------

    fn alloc_bytes(&self, size: usize) -> Result<DeviceBuffer, GpuError> {
        // CL_MEM_READ_WRITE: accessible for both kernel reads and writes
        let buf: ClBuffer<u8> = unsafe {
            ClBuffer::create(&self.context, CL_MEM_READ_WRITE, size, std::ptr::null_mut())
                .map_err(|e| ocl_err(format!("clCreateBuffer({} bytes) failed: {:?}", size, e)))?
        };

        Ok(Arc::new(OpenCLBuffer {
            buf: Mutex::new(buf),
            size_bytes: size,
        }))
    }

    // ---- Synchronous transfers -------------------------------------------

    fn htod_sync(
        &self,
        src: *const c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
    ) -> Result<(), GpuError> {
        let dst_buf = dst_as_opencl(dst)?;

        let queue = self.default_queue.lock().unwrap();

        // clEnqueueWriteBuffer: (buf, blocking, offset, data: &[u8], wait_list)
        let data = unsafe { std::slice::from_raw_parts(src as *const u8, src_bytes) };
        unsafe {
            queue
                .enqueue_write_buffer(
                    &mut *dst_buf.buf.lock().unwrap(),
                    opencl3::types::CL_TRUE,
                    0,
                    data,
                    &[],
                )
                .map_err(|e| ocl_err(format!("enqueue_write_buffer (sync) failed: {:?}", e)))?;
        }
        Ok(())
    }

    fn dtoh_sync(
        &self,
        src: &DeviceBuffer,
        dst: *mut c_void,
        dst_bytes: usize,
    ) -> Result<(), GpuError> {
        let src_buf = dst_as_opencl(src)?;

        let queue = self.default_queue.lock().unwrap();

        // clEnqueueReadBuffer: (buf, blocking, offset, data: &mut [u8], wait_list)
        let data = unsafe { std::slice::from_raw_parts_mut(dst as *mut u8, dst_bytes) };
        unsafe {
            queue
                .enqueue_read_buffer(
                    &mut *src_buf.buf.lock().unwrap(),
                    opencl3::types::CL_TRUE,
                    0,
                    data,
                    &[],
                )
                .map_err(|e| ocl_err(format!("enqueue_read_buffer (sync) failed: {:?}", e)))?;
        }
        Ok(())
    }

    // ---- Asynchronous transfers ------------------------------------------

    unsafe fn htod_async(
        &self,
        src: *const c_void,
        src_bytes: usize,
        dst: &DeviceBuffer,
        stream: &DeviceStream,
    ) -> Result<(), GpuError> {
        let dst_buf = dst_as_opencl(dst)?;
        let queue = stream_as_opencl(stream)?;

        // CL_FALSE = non-blocking write
        let data = std::slice::from_raw_parts(src as *const u8, src_bytes);
        queue
            .enqueue_write_buffer(
                &mut *dst_buf.buf.lock().unwrap(),
                opencl3::types::CL_FALSE,
                0,
                data,
                &[],
            )
            .map_err(|e| ocl_err(format!("enqueue_write_buffer (async) failed: {:?}", e)))?;

        Ok(())
    }

    unsafe fn dtoh_async(
        &self,
        src: &DeviceBuffer,
        dst: *mut c_void,
        dst_bytes: usize,
        stream: &DeviceStream,
    ) -> Result<(), GpuError> {
        let src_buf = dst_as_opencl(src)?;
        let queue = stream_as_opencl(stream)?;

        let data = std::slice::from_raw_parts_mut(dst as *mut u8, dst_bytes);
        queue
            .enqueue_read_buffer(
                &mut *src_buf.buf.lock().unwrap(),
                opencl3::types::CL_FALSE,
                0,
                data,
                &[],
            )
            .map_err(|e| ocl_err(format!("enqueue_read_buffer (async) failed: {:?}", e)))?;

        Ok(())
    }

    // ---- Streams ---------------------------------------------------------

    fn create_stream(&self) -> Result<DeviceStream, GpuError> {
        let id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);

        // Each "stream" is an independent in-order command queue.
        // Using CL_QUEUE_PROFILING_ENABLE allows timing queries.
        let queue = CommandQueue::create_default_with_properties(
            &self.context,
            CL_QUEUE_PROFILING_ENABLE,
            0,
        )
        .map_err(|e| ocl_err(format!("create_stream CommandQueue failed: {:?}", e)))?;

        Ok(Arc::new(OpenCLStream { queue, id }))
    }

    // ---- Device sync -----------------------------------------------------

    fn synchronize_device(&self) -> Result<(), GpuError> {
        self.default_queue
            .lock()
            .unwrap()
            .finish()
            .map_err(|e| ocl_err(format!("synchronize_device finish() failed: {:?}", e)))
    }

    // ---- Memory info -----------------------------------------------------

    fn memory_info(&self) -> Result<(usize, usize), GpuError> {
        // CL_DEVICE_GLOBAL_MEM_SIZE
        let total = self
            .device
            .global_mem_size()
            .map_err(|e| ocl_err(format!("global_mem_size failed: {:?}", e)))?
            as usize;

        // OpenCL has no direct "free memory" query; return total for both.
        Ok((total, total))
    }
}

// ---------------------------------------------------------------------------
// Downcast helpers
// ---------------------------------------------------------------------------

fn dst_as_opencl(buf: &DeviceBuffer) -> Result<&OpenCLBuffer, GpuError> {
    buf.as_any()
        .downcast_ref::<OpenCLBuffer>()
        .ok_or_else(|| ocl_err("DeviceBuffer is not an OpenCL buffer"))
}

fn stream_as_opencl(stream: &DeviceStream) -> Result<&CommandQueue, GpuError> {
    stream
        .as_any()
        .downcast_ref::<OpenCLStream>()
        .map(|s| &s.queue)
        .ok_or_else(|| ocl_err("DeviceStream is not an OpenCL stream"))
}
