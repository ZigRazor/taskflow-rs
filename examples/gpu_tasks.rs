use std::sync::Arc;
use taskflow_rs::{Executor, GpuBuffer, GpuDevice, GpuTaskConfig, Taskflow};

fn main() {
    println!("=== GPU Tasks Demo ===\n");

    // Check if GPU is available
    match GpuDevice::new(0) {
        Ok(device) => {
            println!("✓ CUDA device initialized successfully\n");

            demo_gpu_cpu_pipeline(device.clone());
            println!();

            demo_data_transfer(device.clone());
            println!();

            demo_heterogeneous_workflow(device);
        }
        Err(e) => {
            println!("✗ Failed to initialize CUDA device: {}", e);
            println!("  Make sure you have:");
            println!("  1. NVIDIA GPU with CUDA support");
            println!("  2. CUDA toolkit installed");
            println!("  3. Proper drivers configured");
        }
    }
}

/// Demo 1: GPU-CPU Pipeline
fn demo_gpu_cpu_pipeline(device: GpuDevice) {
    println!("1. GPU-CPU Pipeline");
    println!("   Processing data through GPU and CPU stages\n");

    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    // CPU task: Generate data
    let data = Arc::new(std::sync::Mutex::new(Vec::new()));
    let d1 = data.clone();

    let generate = taskflow.emplace(move || {
        println!("   [CPU] Generating data...");
        let mut data = d1.lock().unwrap();
        *data = (0..1024).map(|i| i as f32).collect();
        println!("   [CPU] Generated {} elements", data.len());
    });

    // GPU task: Process on GPU (simulated)
    let d2 = data.clone();
    let dev = device.clone();

    let process_gpu = taskflow.emplace(move || {
        println!("   [GPU] Processing on CUDA device...");

        let data = d2.lock().unwrap();

        // Allocate GPU buffers
        let mut input_buf =
            GpuBuffer::allocate(&dev, data.len()).expect("Failed to allocate input buffer");
        let mut output_buf =
            GpuBuffer::allocate(&dev, data.len()).expect("Failed to allocate output buffer");

        // Copy data to GPU
        input_buf
            .copy_from_host(&data)
            .expect("Failed to copy to GPU");

        println!("   [GPU] Data transferred to device");
        println!("   [GPU] Running kernel (simulated)...");

        dev.synchronize().expect("GPU sync failed");

        println!("   [GPU] Kernel completed");

        // Copy results back
        let mut results = vec![0.0f32; data.len()];
        output_buf
            .copy_to_host(&mut results)
            .expect("Failed to copy from GPU");

        println!("   [GPU] Results transferred back to host");
    });

    // CPU task: Validate results
    let d3 = data.clone();
    let validate = taskflow.emplace(move || {
        println!("   [CPU] Validating results...");
        let data = d3.lock().unwrap();
        println!("   [CPU] Validated {} elements", data.len());
    });

    // Set up pipeline
    generate.precede(&process_gpu);
    process_gpu.precede(&validate);

    executor.run(&taskflow).wait();

    println!("   ✓ GPU-CPU pipeline complete");
}

/// Demo 2: Data Transfer Management
fn demo_data_transfer(device: GpuDevice) {
    println!("2. Data Transfer Management");
    println!("   Demonstrating host-device memory operations\n");

    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    let host_data = Arc::new(std::sync::Mutex::new(vec![0.0f32; 1024]));

    // Initialize data on host
    let h1 = host_data.clone();
    let init = taskflow.emplace(move || {
        let mut data = h1.lock().unwrap();
        for (i, val) in data.iter_mut().enumerate() {
            *val = (i as f32) * 2.0;
        }
        println!("   [CPU] Initialized {} elements on host", data.len());
    });

    // Transfer to GPU
    let h2 = host_data.clone();
    let dev = device.clone();
    let to_gpu = taskflow.emplace(move || {
        let data = h2.lock().unwrap();

        let mut gpu_buf = GpuBuffer::allocate(&dev, data.len()).expect("Allocation failed");

        let start = std::time::Instant::now();
        gpu_buf.copy_from_host(&data).expect("H2D copy failed");
        let elapsed = start.elapsed();

        let bandwidth = (data.len() * 4) as f64 / elapsed.as_secs_f64() / 1e9;
        println!("   [Transfer] Host→Device: {:.2} GB/s", bandwidth);
    });

    init.precede(&to_gpu);

    executor.run(&taskflow).wait();

    println!("   ✓ Data transfer complete");
}

/// Demo 3: Heterogeneous Workflow
fn demo_heterogeneous_workflow(device: GpuDevice) {
    println!("3. Heterogeneous Workflow");
    println!("   Mix of CPU and GPU tasks in DAG\n");

    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    // CPU preprocessing
    let preprocess = taskflow.emplace(|| {
        println!("   [CPU] Preprocessing data...");
        std::thread::sleep(std::time::Duration::from_millis(100));
    });

    // Parallel GPU tasks
    let dev1 = device.clone();
    let gpu_task_1 = taskflow.emplace(move || {
        println!("   [GPU-0] Computing on device...");
        let config = GpuTaskConfig::linear(1024, 256);
        println!(
            "   [GPU-0] Launch config: blocks={}, threads={}",
            config.grid_dim.0, config.block_dim.0
        );
        dev1.synchronize().ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
    });

    let dev2 = device.clone();
    let gpu_task_2 = taskflow.emplace(move || {
        println!("   [GPU-1] Computing on device...");
        dev2.synchronize().ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
    });

    // CPU postprocessing
    let postprocess = taskflow.emplace(|| {
        println!("   [CPU] Postprocessing results...");
        std::thread::sleep(std::time::Duration::from_millis(100));
    });

    // Build DAG
    preprocess.precede(&gpu_task_1);
    preprocess.precede(&gpu_task_2);
    gpu_task_1.precede(&postprocess);
    gpu_task_2.precede(&postprocess);

    println!("   Executing heterogeneous workflow...\n");
    executor.run(&taskflow).wait();

    println!("   ✓ Heterogeneous workflow complete");
}
