// GPU Support Module
// This module provides CUDA integration when compiled with --features gpu

#[cfg(feature = "gpu")]
use cudarc::driver::{CudaDevice, CudaSlice, LaunchConfig, ValidAsZeroBits, DeviceRepr};
#[cfg(feature = "gpu")]
use std::sync::Arc;

/// GPU device handle for CUDA operations
#[cfg(feature = "gpu")]
pub struct GpuDevice {
    device: Arc<CudaDevice>,
}

/// GPU device handle (stub when GPU feature is disabled)
#[cfg(not(feature = "gpu"))]
pub struct GpuDevice;

#[cfg(feature = "gpu")]
impl GpuDevice {
    /// Create a new GPU device handle
    /// 
    /// # Arguments
    /// * `device_id` - CUDA device ID (0 for first GPU)
    pub fn new(device_id: usize) -> Result<Self, String> {
        let device = CudaDevice::new(device_id)
            .map_err(|e| format!("Failed to initialize CUDA device {}: {:?}", device_id, e))?;
        
        Ok(Self {
            device,
        })
    }
    
    /// Get the underlying CUDA device
    pub fn device(&self) -> &Arc<CudaDevice> {
        &self.device
    }
    
    /// Synchronize the device (wait for all operations to complete)
    pub fn synchronize(&self) -> Result<(), String> {
        self.device.synchronize()
            .map_err(|e| format!("Device synchronization failed: {:?}", e))
    }
}

#[cfg(feature = "gpu")]
impl Clone for GpuDevice {
    fn clone(&self) -> Self {
        Self {
            device: Arc::clone(&self.device),
        }
    }
}

#[cfg(not(feature = "gpu"))]
impl GpuDevice {
    /// Returns an error when GPU feature is not enabled
    pub fn new(_device_id: usize) -> Result<Self, String> {
        Err("GPU support not compiled. Build with --features gpu".to_string())
    }
    
    /// Stub synchronize method
    pub fn synchronize(&self) -> Result<(), String> {
        Err("GPU support not compiled".to_string())
    }
}

#[cfg(not(feature = "gpu"))]
impl Clone for GpuDevice {
    fn clone(&self) -> Self {
        Self
    }
}

/// GPU buffer for data transfer between host and device
#[cfg(feature = "gpu")]
pub struct GpuBuffer<T: DeviceRepr + ValidAsZeroBits> {
    device_data: CudaSlice<T>,
    device: Arc<CudaDevice>,
}

/// GPU buffer (stub when GPU feature is disabled)
#[cfg(not(feature = "gpu"))]
pub struct GpuBuffer<T> {
    _phantom: std::marker::PhantomData<T>,
}

#[cfg(feature = "gpu")]
impl<T: DeviceRepr + ValidAsZeroBits> GpuBuffer<T> {
    /// Allocate a buffer on the GPU
    pub fn allocate(device: &GpuDevice, size: usize) -> Result<Self, String> {
        let device_data = device.device.alloc_zeros::<T>(size)
            .map_err(|e| format!("Failed to allocate GPU memory: {:?}", e))?;
        
        Ok(Self {
            device_data,
            device: Arc::clone(&device.device),
        })
    }
    
    /// Copy data from host to device
    pub fn copy_from_host(&mut self, host_data: &[T]) -> Result<(), String> {
        self.device.htod_sync_copy_into(host_data, &mut self.device_data)
            .map_err(|e| format!("Host-to-device copy failed: {:?}", e))
    }
    
    /// Copy data from device to host
    pub fn copy_to_host(&self, host_data: &mut [T]) -> Result<(), String> {
        self.device.dtoh_sync_copy_into(&self.device_data, host_data)
            .map_err(|e| format!("Device-to-host copy failed: {:?}", e))
    }
    
    /// Get the device slice
    pub fn device_slice(&self) -> &CudaSlice<T> {
        &self.device_data
    }
    
    /// Get mutable device slice
    pub fn device_slice_mut(&mut self) -> &mut CudaSlice<T> {
        &mut self.device_data
    }
}

#[cfg(not(feature = "gpu"))]
impl<T> GpuBuffer<T> {
    /// Stub allocate - returns error when GPU feature is not enabled
    pub fn allocate(_device: &GpuDevice, _size: usize) -> Result<Self, String> {
        Err("GPU support not compiled. Build with --features gpu".to_string())
    }
    
    /// Stub copy_from_host
    pub fn copy_from_host(&mut self, _host_data: &[T]) -> Result<(), String> {
        Err("GPU support not compiled".to_string())
    }
    
    /// Stub copy_to_host
    pub fn copy_to_host(&self, _host_data: &mut [T]) -> Result<(), String> {
        Err("GPU support not compiled".to_string())
    }
}

/// GPU task configuration
#[cfg(feature = "gpu")]
pub struct GpuTaskConfig {
    /// Number of thread blocks
    pub grid_dim: (u32, u32, u32),
    /// Number of threads per block
    pub block_dim: (u32, u32, u32),
    /// Shared memory size in bytes
    pub shared_mem_bytes: u32,
}

/// GPU task configuration (stub when GPU feature is disabled)
#[cfg(not(feature = "gpu"))]
pub struct GpuTaskConfig {
    pub grid_dim: (u32, u32, u32),
    pub block_dim: (u32, u32, u32),
    pub shared_mem_bytes: u32,
}

#[cfg(feature = "gpu")]
impl Default for GpuTaskConfig {
    fn default() -> Self {
        Self {
            grid_dim: (1, 1, 1),
            block_dim: (256, 1, 1),
            shared_mem_bytes: 0,
        }
    }
}

#[cfg(feature = "gpu")]
impl GpuTaskConfig {
    /// Create a 1D launch configuration
    pub fn linear(num_elements: usize, threads_per_block: u32) -> Self {
        let num_blocks = (num_elements as u32 + threads_per_block - 1) / threads_per_block;
        Self {
            grid_dim: (num_blocks, 1, 1),
            block_dim: (threads_per_block, 1, 1),
            shared_mem_bytes: 0,
        }
    }
    
    /// Create a 2D launch configuration
    pub fn grid_2d(width: u32, height: u32, block_size: (u32, u32)) -> Self {
        let grid_x = (width + block_size.0 - 1) / block_size.0;
        let grid_y = (height + block_size.1 - 1) / block_size.1;
        
        Self {
            grid_dim: (grid_x, grid_y, 1),
            block_dim: (block_size.0, block_size.1, 1),
            shared_mem_bytes: 0,
        }
    }
    
    /// Convert to LaunchConfig
    pub fn to_launch_config(&self) -> LaunchConfig {
        LaunchConfig {
            grid_dim: self.grid_dim,
            block_dim: self.block_dim,
            shared_mem_bytes: self.shared_mem_bytes,
        }
    }
}

#[cfg(not(feature = "gpu"))]
impl Default for GpuTaskConfig {
    fn default() -> Self {
        Self {
            grid_dim: (1, 1, 1),
            block_dim: (256, 1, 1),
            shared_mem_bytes: 0,
        }
    }
}

#[cfg(not(feature = "gpu"))]
impl GpuTaskConfig {
    /// Stub 1D launch configuration
    pub fn linear(num_elements: usize, threads_per_block: u32) -> Self {
        let num_blocks = (num_elements as u32 + threads_per_block - 1) / threads_per_block;
        Self {
            grid_dim: (num_blocks, 1, 1),
            block_dim: (threads_per_block, 1, 1),
            shared_mem_bytes: 0,
        }
    }
    
    /// Stub 2D launch configuration
    pub fn grid_2d(width: u32, height: u32, block_size: (u32, u32)) -> Self {
        let grid_x = (width + block_size.0 - 1) / block_size.0;
        let grid_y = (height + block_size.1 - 1) / block_size.1;
        
        Self {
            grid_dim: (grid_x, grid_y, 1),
            block_dim: (block_size.0, block_size.1, 1),
            shared_mem_bytes: 0,
        }
    }
}

/// Example CUDA kernel (in real use, this would be compiled PTX)
#[cfg(feature = "gpu")]
pub const VECTOR_ADD_PTX: &str = r#"
.version 7.0
.target sm_52
.address_size 64

.visible .entry vector_add(
    .param .u64 vector_add_param_0,
    .param .u64 vector_add_param_1,
    .param .u64 vector_add_param_2,
    .param .u32 vector_add_param_3
)
{
    .reg .pred  %p<2>;
    .reg .f32   %f<4>;
    .reg .b32   %r<6>;
    .reg .b64   %rd<11>;

    ld.param.u64    %rd1, [vector_add_param_0];
    ld.param.u64    %rd2, [vector_add_param_1];
    ld.param.u64    %rd3, [vector_add_param_2];
    ld.param.u32    %r2, [vector_add_param_3];
    mov.u32     %r3, %ctaid.x;
    mov.u32     %r4, %ntid.x;
    mov.u32     %r5, %tid.x;
    mad.lo.s32  %r1, %r3, %r4, %r5;
    setp.ge.s32 %p1, %r1, %r2;
    @%p1 bra    BB0_2;

    cvta.to.global.u64  %rd4, %rd1;
    mul.wide.s32    %rd5, %r1, 4;
    add.s64     %rd6, %rd4, %rd5;
    cvta.to.global.u64  %rd7, %rd2;
    add.s64     %rd8, %rd7, %rd5;
    ld.global.f32   %f1, [%rd8];
    ld.global.f32   %f2, [%rd6];
    add.f32     %f3, %f2, %f1;
    cvta.to.global.u64  %rd9, %rd3;
    add.s64     %rd10, %rd9, %rd5;
    st.global.f32   [%rd10], %f3;

BB0_2:
    ret;
}
"#;

#[cfg(test)]
#[cfg(feature = "gpu")]
mod tests {
    use super::*;
    
    #[test]
    #[ignore] // Only run with GPU available
    fn test_gpu_device_init() {
        let device = GpuDevice::new(0);
        assert!(device.is_ok());
    }
    
    #[test]
    #[ignore]
    fn test_gpu_buffer_allocation() {
        let device = GpuDevice::new(0).unwrap();
        let buffer: Result<GpuBuffer<f32>, _> = GpuBuffer::allocate(&device, 1024);
        assert!(buffer.is_ok());
    }
}
