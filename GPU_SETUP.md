# GPU Setup Guide

Quick guide to get GPU support working in TaskFlow-RS.

## Step 1: Check if you have CUDA

```bash
# Check CUDA version
nvcc --version

# Check GPU
nvidia-smi
```

If both commands work, note your CUDA version (e.g., "release 11.8" or "release 12.0").

## Step 2: Configure CUDA Version

The default configuration uses **CUDA 12.0**. If you have a different version:

1. Open `Cargo.toml`
2. Find this line (around line 22):
   ```toml
   gpu = ["cudarc", "cudarc/cuda-12000"]
   ```

3. Comment it out and uncomment the line for your CUDA version:
   ```toml
   # gpu = ["cudarc", "cudarc/cuda-12000"]  # CUDA 12.0 (default)
   gpu = ["cudarc", "cudarc/cuda-11080"]   # CUDA 11.8
   ```

**Available versions:**
- `cudarc/cuda-11040` - CUDA 11.4
- `cudarc/cuda-11050` - CUDA 11.5
- `cudarc/cuda-11060` - CUDA 11.6
- `cudarc/cuda-11070` - CUDA 11.7
- `cudarc/cuda-11080` - CUDA 11.8
- `cudarc/cuda-12000` - CUDA 12.0 (default)
- `cudarc/cuda-12010` - CUDA 12.1
- `cudarc/cuda-12020` - CUDA 12.2
- `cudarc/cuda-12030` - CUDA 12.3
- `cudarc/cuda-12040` - CUDA 12.4
- `cudarc/cuda-12050` - CUDA 12.5

## Step 3: Build with GPU Support

```bash
cargo build --features gpu
```

## Step 4: Run Examples

```bash
# GPU tasks example
cargo run --features gpu --example gpu_tasks

# GPU pipeline example
cargo run --features gpu --example gpu_pipeline
```

## Don't Have CUDA?

That's fine! GPU support is optional:

```bash
# Build without GPU
cargo build

# Examples will detect missing GPU gracefully
cargo run --example gpu_tasks
# Output: "✗ GPU not available: Failed to initialize CUDA device"
```

## Troubleshooting

### "Must specify one of the following features"

You need to configure the CUDA version in Cargo.toml (see Step 2 above).

### "CUDA device not found"

1. Make sure your GPU is working: `nvidia-smi`
2. Install/update NVIDIA drivers
3. Install CUDA toolkit: `sudo apt install nvidia-cuda-toolkit`

### "version mismatch"

Your CUDA runtime version doesn't match the version specified in Cargo.toml. Update the feature in Cargo.toml to match your `nvcc --version` output.

### Still not working?

1. Check CUDA is in your PATH:
   ```bash
   echo $CUDA_HOME
   # Should show something like /usr/local/cuda
   ```

2. Set CUDA_HOME if needed:
   ```bash
   export CUDA_HOME=/usr/local/cuda
   export PATH=$CUDA_HOME/bin:$PATH
   export LD_LIBRARY_PATH=$CUDA_HOME/lib64:$LD_LIBRARY_PATH
   ```

3. Verify shared libraries:
   ```bash
   ldconfig -p | grep cuda
   ```

## Quick Test

Test if everything works:

```rust
use taskflow_rs::GpuDevice;

fn main() {
    match GpuDevice::new(0) {
        Ok(device) => {
            println!("✓ GPU initialized successfully!");
            device.synchronize().ok();
        }
        Err(e) => {
            println!("✗ GPU failed: {}", e);
        }
    }
}
```

Save as `test_gpu.rs` and run:
```bash
cargo run --features gpu --bin test_gpu
```
