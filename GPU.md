# GPU Support

TaskFlow-RS provides CUDA integration for heterogeneous computing, allowing GPU tasks to be seamlessly integrated into task graphs alongside CPU tasks.

## Features

- **CUDA Integration** - Native CUDA support via cudarc
- **Data Transfer Management** - Efficient host-device memory operations
- **GPU-CPU Synchronization** - Proper task ordering and synchronization
- **Heterogeneous Workflows** - Mix CPU and GPU tasks in the same DAG
- **Memory Management** - Automatic GPU memory handling

## Prerequisites

### Hardware
- NVIDIA GPU with CUDA Compute Capability 5.2 or higher
- Minimum 2GB GPU memory recommended

### Software
1. **CUDA Toolkit** (11.0 or higher)
   ```bash
   # Ubuntu/Debian
   sudo apt install nvidia-cuda-toolkit
   
   # Check installation and version
   nvcc --version
   ```

2. **NVIDIA Drivers**
   ```bash
   # Check driver version
   nvidia-smi
   ```

3. **Rust with GPU feature**
   
   **Important**: Check your CUDA version first, then configure if needed.
   
   ```bash
   # Check your CUDA version
   nvcc --version
   # Look for "release X.Y" in the output
   ```
   
   **If you have CUDA 12.0** (default):
   ```bash
   cargo build --features gpu
   ```
   
   **If you have a different CUDA version** (e.g., 11.8 or 12.5):
   1. Edit `Cargo.toml`
   2. In the `[features]` section, find the line starting with `gpu =`
   3. Change `cudarc/cuda-12000` to match your version
   4. Example for CUDA 11.8: `gpu = ["cudarc", "cudarc/cuda-11080"]`
   5. Then build: `cargo build --features gpu`
   
   See `GPU_SETUP.md` for detailed version configuration instructions.

### Optional: Building Without GPU

If you don't have CUDA installed, simply don't use the `--features gpu` flag:

```bash
# Build without GPU support
cargo build

# Examples will gracefully handle missing GPU
cargo run --example gpu_tasks
# Output: "✗ Failed to initialize CUDA device: ..."
```

## Basic Usage

### Initialize GPU Device

```rust
use taskflow_rs::GpuDevice;

// Initialize first GPU (device 0)
let device = GpuDevice::new(0)
    .expect("Failed to initialize CUDA device");

// Synchronize device
device.synchronize()
    .expect("Synchronization failed");
```

### GPU Buffers

```rust
use taskflow_rs::{GpuDevice, GpuBuffer};

let device = GpuDevice::new(0).unwrap();

// Allocate buffer on GPU
let mut gpu_buffer: GpuBuffer<f32> = GpuBuffer::allocate(&device, 1024)
    .expect("Allocation failed");

// Host data
let host_data: Vec<f32> = (0..1024).map(|i| i as f32).collect();

// Copy host → device
gpu_buffer.copy_from_host(&host_data)
    .expect("H2D copy failed");

// Copy device → host
let mut results = vec![0.0f32; 1024];
gpu_buffer.copy_to_host(&mut results)
    .expect("D2H copy failed");
```

### GPU-CPU Pipeline

```rust
use taskflow_rs::{Executor, Taskflow, GpuDevice, GpuBuffer};
use std::sync::Arc;

let mut executor = Executor::new(4);
let mut taskflow = Taskflow::new();
let device = GpuDevice::new(0).unwrap();

let data = Arc::new(std::sync::Mutex::new(Vec::new()));

// CPU task: Generate data
let d1 = data.clone();
let generate = taskflow.emplace(move || {
    let mut data = d1.lock().unwrap();
    *data = (0..1024).map(|i| i as f32).collect();
});

// GPU task: Process on device
let d2 = data.clone();
let dev = device.clone();
let process_gpu = taskflow.emplace(move || {
    let data = d2.lock().unwrap();
    
    // Allocate and transfer
    let mut input = GpuBuffer::allocate(&dev, data.len()).unwrap();
    input.copy_from_host(&data).unwrap();
    
    // Run computation (kernel launch would go here)
    dev.synchronize().unwrap();
    
    // Transfer back
    let mut output = vec![0.0f32; data.len()];
    input.copy_to_host(&mut output).unwrap();
});

// CPU task: Validate
let d3 = data.clone();
let validate = taskflow.emplace(move || {
    let data = d3.lock().unwrap();
    println!("Validated {} elements", data.len());
});

// Build pipeline
generate.precede(&process_gpu);
process_gpu.precede(&validate);

executor.run(&taskflow).wait();
```

## Advanced Patterns

### Pattern 1: Heterogeneous Workflow

Mix CPU and GPU tasks in a complex DAG:

```rust
let mut taskflow = Taskflow::new();
let device = GpuDevice::new(0).unwrap();

// CPU preprocessing
let preprocess = taskflow.emplace(|| {
    println!("CPU preprocessing");
});

// Parallel GPU computations
let dev1 = device.clone();
let gpu_task_1 = taskflow.emplace(move || {
    println!("GPU computation 1");
    dev1.synchronize().ok();
});

let dev2 = device.clone();
let gpu_task_2 = taskflow.emplace(move || {
    println!("GPU computation 2");
    dev2.synchronize().ok();
});

// CPU postprocessing
let postprocess = taskflow.emplace(|| {
    println!("CPU postprocessing");
});

// Build DAG: preprocess → [gpu1, gpu2] → postprocess
preprocess.precede(&gpu_task_1);
preprocess.precede(&gpu_task_2);
gpu_task_1.precede(&postprocess);
gpu_task_2.precede(&postprocess);
```

### Pattern 2: GPU Data Pipeline

Process streaming data through GPU:

```rust
use taskflow_rs::GpuTaskConfig;

let device = GpuDevice::new(0).unwrap();

for batch in data_batches {
    let mut taskflow = Taskflow::new();
    let dev = device.clone();
    
    let process = taskflow.emplace(move || {
        // Configure kernel launch
        let config = GpuTaskConfig::linear(batch.len(), 256);
        
        // Allocate buffers
        let mut input = GpuBuffer::allocate(&dev, batch.len()).unwrap();
        let mut output = GpuBuffer::allocate(&dev, batch.len()).unwrap();
        
        // Transfer and compute
        input.copy_from_host(&batch).unwrap();
        // Launch kernel here
        dev.synchronize().unwrap();
        
        // Transfer results
        let mut results = vec![0.0; batch.len()];
        output.copy_to_host(&mut results).unwrap();
    });
    
    executor.run(&taskflow).wait();
}
```

### Pattern 3: Multi-GPU

Use multiple GPUs in parallel:

```rust
let device_0 = GpuDevice::new(0).unwrap();
let device_1 = GpuDevice::new(1).unwrap();

let mut taskflow = Taskflow::new();

// Task on GPU 0
let dev0 = device_0.clone();
let task_gpu0 = taskflow.emplace(move || {
    println!("Running on GPU 0");
    // Compute on device 0
    dev0.synchronize().ok();
});

// Task on GPU 1
let dev1 = device_1.clone();
let task_gpu1 = taskflow.emplace(move || {
    println!("Running on GPU 1");
    // Compute on device 1
    dev1.synchronize().ok();
});

// Run both in parallel
executor.run(&taskflow).wait();
```

## Launch Configuration

Configure kernel launches with `GpuTaskConfig`:

```rust
use taskflow_rs::GpuTaskConfig;

// 1D configuration (common for vector operations)
let config = GpuTaskConfig::linear(
    1024,  // number of elements
    256    // threads per block
);
// Result: 4 blocks × 256 threads = 1024 threads

// 2D configuration (common for image processing)
let config = GpuTaskConfig::grid_2d(
    1920,        // width
    1080,        // height
    (16, 16)     // block size
);
// Result: 120×68 blocks × 16×16 threads

// Custom configuration
let config = GpuTaskConfig {
    grid_dim: (100, 1, 1),
    block_dim: (256, 1, 1),
    shared_mem_bytes: 4096,
};
```

## Data Transfer Optimization

### Pinned Memory

For faster transfers, use pinned (page-locked) memory:

```rust
// TODO: Implement pinned memory support
// For now, use standard allocations
let host_data = vec![0.0f32; size];
```

### Asynchronous Transfers

Overlap computation with data transfer:

```rust
// TODO: Implement async transfer support
// Currently all transfers are synchronous
```

### Minimize Transfers

Keep data on GPU when possible:

```rust
// Bad: Transfer back and forth
gpu_buffer.copy_to_host(&mut temp).unwrap();
// Do CPU work
gpu_buffer.copy_from_host(&temp).unwrap();

// Good: Keep on GPU, chain GPU operations
// Computation 1 → Computation 2 → ... → Final transfer
```

## Error Handling

All GPU operations return `Result` for proper error handling:

```rust
use taskflow_rs::{GpuDevice, GpuBuffer};

match GpuDevice::new(0) {
    Ok(device) => {
        match GpuBuffer::<f32>::allocate(&device, 1024) {
            Ok(buffer) => {
                // Use buffer
            }
            Err(e) => eprintln!("Allocation failed: {}", e),
        }
    }
    Err(e) => eprintln!("Device init failed: {}", e),
}
```

## Performance Considerations

### Transfer Overhead

- **H2D (Host-to-Device)**: ~10 GB/s typical
- **D2H (Device-to-Host)**: ~10 GB/s typical
- **Compute**: 100x-1000x faster than CPU for parallel workloads

### When to Use GPU

✅ **Good for GPU:**
- Large data parallel operations
- Matrix operations (GEMM)
- Image/signal processing
- Monte Carlo simulations

❌ **Not ideal for GPU:**
- Small datasets (< 10K elements)
- Sequential algorithms
- Frequent small transfers
- Complex branching logic

### Optimization Tips

1. **Batch Operations**: Process multiple items per kernel launch
2. **Minimize Transfers**: Keep data on GPU between operations
3. **Use Async**: Overlap CPU and GPU work
4. **Coalesce Memory**: Access memory in coalesced patterns
5. **Occupancy**: Tune block/grid size for your GPU

## Debugging

### Check GPU Status

```bash
# Monitor GPU usage
nvidia-smi

# Watch continuously
watch -n 1 nvidia-smi
```

### Common Issues

**Issue**: "Must specify one of the following features: [cuda-version-from-build-system, cuda-12050, ...]"
- **Cause**: cudarc requires explicit CUDA version
- **Solution**: The default is CUDA 12.0. If you have a different version:
  1. Check your version: `nvcc --version`
  2. Edit `Cargo.toml` in the `[features]` section (around line 22)
  3. Find: `gpu = ["cudarc", "cudarc/cuda-12000"]`
  4. Change to match your version, e.g.: `gpu = ["cudarc", "cudarc/cuda-11080"]` for CUDA 11.8
  5. See GPU_SETUP.md for complete list of versions

**Issue**: "CUDA device not found"
- Check: `nvidia-smi` shows GPU
- Install: NVIDIA drivers
- Verify: CUDA toolkit installed

**Issue**: "Out of memory"
- Reduce batch size
- Free unused buffers
- Use smaller data types (f16 vs f32)

**Issue**: "Kernel launch failed"
- Check launch configuration
- Verify grid/block dimensions
- Check shared memory size

## Examples

### Run GPU Examples

```bash
# Build with GPU support
cargo build --features gpu --release

# Run GPU tasks example
cargo run --features gpu --example gpu_tasks

# Run without GPU (falls back gracefully)
cargo run --example gpu_tasks
```

### Example: Vector Addition

```rust
use taskflow_rs::{Executor, Taskflow, GpuDevice, GpuBuffer};

fn vector_add_gpu() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    let device = GpuDevice::new(0).unwrap();
    
    let size = 1_000_000;
    
    // Generate input vectors
    let a: Vec<f32> = (0..size).map(|i| i as f32).collect();
    let b: Vec<f32> = (0..size).map(|i| (i * 2) as f32).collect();
    
    let dev = device.clone();
    let compute = taskflow.emplace(move || {
        // Allocate GPU memory
        let mut d_a = GpuBuffer::allocate(&dev, size).unwrap();
        let mut d_b = GpuBuffer::allocate(&dev, size).unwrap();
        let mut d_c = GpuBuffer::allocate(&dev, size).unwrap();
        
        // Transfer to GPU
        d_a.copy_from_host(&a).unwrap();
        d_b.copy_from_host(&b).unwrap();
        
        // Launch kernel (would use actual kernel here)
        // For now, simulate with synchronize
        dev.synchronize().unwrap();
        
        // Transfer result back
        let mut c = vec![0.0f32; size];
        d_c.copy_to_host(&mut c).unwrap();
        
        println!("Computed {} elements on GPU", size);
    });
    
    executor.run(&taskflow).wait();
}
```

## API Reference

### GpuDevice

```rust
impl GpuDevice {
    pub fn new(device_id: usize) -> Result<Self, String>;
    pub fn device(&self) -> &Arc<CudaDevice>;
    pub fn synchronize(&self) -> Result<(), String>;
}
```

### GpuBuffer

```rust
impl<T: DeviceRepr> GpuBuffer<T> {
    pub fn allocate(device: &GpuDevice, size: usize) -> Result<Self, String>;
    pub fn copy_from_host(&mut self, host_data: &[T]) -> Result<(), String>;
    pub fn copy_to_host(&self, host_data: &mut [T]) -> Result<(), String>;
    pub fn device_slice(&self) -> &CudaSlice<T>;
    pub fn device_slice_mut(&mut self) -> &mut CudaSlice<T>;
}
```

### GpuTaskConfig

```rust
impl GpuTaskConfig {
    pub fn linear(num_elements: usize, threads_per_block: u32) -> Self;
    pub fn grid_2d(width: u32, height: u32, block_size: (u32, u32)) -> Self;
    pub fn to_launch_config(&self) -> LaunchConfig;
}
```

## Limitations

1. **CUDA Only**: Currently only NVIDIA CUDA is supported (no OpenCL, ROCm, Metal)
2. **Synchronous Transfers**: All transfers are currently blocking
3. **No Unified Memory**: Must manually manage host/device memory
4. **Single Stream**: All operations use default CUDA stream

## Future Enhancements

- Asynchronous data transfers
- Multiple CUDA streams
- Unified memory support
- OpenCL backend
- ROCm (AMD) support
- Automatic kernel generation
- Performance profiling integration
