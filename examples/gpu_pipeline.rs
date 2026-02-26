use taskflow_rs::{Executor, Taskflow, GpuDevice, GpuBuffer, GpuTaskConfig};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn main() {
    println!("=== GPU Pipeline Example ===\n");
    println!("Simulating image processing pipeline with GPU acceleration\n");
    
    match GpuDevice::new(0) {
        Ok(device) => {
            run_image_pipeline(device);
        }
        Err(e) => {
            println!("✗ GPU not available: {}", e);
            println!("\nThis example requires:");
            println!("  • NVIDIA GPU with CUDA support");
            println!("  • CUDA Toolkit 11.0 or higher");
            println!("  • Run with: cargo run --features gpu --example gpu_pipeline");
        }
    }
}

fn run_image_pipeline(device: GpuDevice) {
    let mut executor = Executor::new(4);
    
    // Simulate processing multiple images
    let num_images = 5;
    let image_size = 1920 * 1080; // Full HD
    
    println!("Processing {} images ({} pixels each)\n", num_images, image_size);
    
    let total_start = Instant::now();
    
    for image_id in 0..num_images {
        let mut taskflow = Taskflow::new();
        let dev = device.clone();
        
        // Shared data between tasks
        let image_data = Arc::new(Mutex::new(Vec::new()));
        
        // Stage 1: Load image (CPU)
        let data1 = image_data.clone();
        let load = taskflow.emplace(move || {
            println!("[Image {}] [CPU] Loading from disk...", image_id);
            let mut data = data1.lock().unwrap();
            
            // Simulate loading image (RGB: 3 channels)
            *data = vec![0.0f32; image_size * 3];
            for i in 0..data.len() {
                data[i] = (i % 256) as f32 / 255.0;
            }
            
            std::thread::sleep(std::time::Duration::from_millis(50));
            println!("[Image {}] [CPU] Loaded {} pixels", image_id, image_size);
        });
        
        // Stage 2: Preprocessing (GPU)
        let data2 = image_data.clone();
        let dev2 = dev.clone();
        let preprocess = taskflow.emplace(move || {
            println!("[Image {}] [GPU] Preprocessing...", image_id);
            let data = data2.lock().unwrap();
            
            let start = Instant::now();
            
            // Allocate GPU memory
            let mut input_buf = GpuBuffer::allocate(&dev2, data.len())
                .expect("Failed to allocate GPU memory");
            
            // Transfer to GPU
            input_buf.copy_from_host(&data)
                .expect("Failed to copy to GPU");
            
            // Configure kernel launch
            let config = GpuTaskConfig::grid_2d(
                1920,      // width
                1080,      // height
                (16, 16)   // block size (common for 2D ops)
            );
            
            println!("[Image {}] [GPU] Launch: {} blocks, {} threads/block", 
                     image_id,
                     config.grid_dim.0 * config.grid_dim.1,
                     config.block_dim.0 * config.block_dim.1);
            
            // Simulate GPU kernel execution
            dev2.synchronize().expect("GPU sync failed");
            
            let elapsed = start.elapsed();
            println!("[Image {}] [GPU] Preprocessed in {:?}", image_id, elapsed);
        });
        
        // Stage 3: Feature extraction (GPU)
        let data3 = image_data.clone();
        let dev3 = dev.clone();
        let extract = taskflow.emplace(move || {
            println!("[Image {}] [GPU] Extracting features...", image_id);
            let data = data3.lock().unwrap();
            
            let start = Instant::now();
            
            // Allocate for features (smaller than image)
            let feature_size = 1000;
            let mut feature_buf = GpuBuffer::allocate(&dev3, feature_size)
                .expect("Failed to allocate features");
            
            // Configure for feature extraction
            let config = GpuTaskConfig::linear(feature_size, 256);
            
            // Simulate GPU computation
            dev3.synchronize().expect("GPU sync failed");
            
            // Transfer features back to host
            let mut features = vec![0.0f32; feature_size];
            feature_buf.copy_to_host(&mut features)
                .expect("Failed to copy features");
            
            let elapsed = start.elapsed();
            println!("[Image {}] [GPU] Extracted {} features in {:?}", 
                     image_id, feature_size, elapsed);
        });
        
        // Stage 4: Classification (CPU)
        let classify = taskflow.emplace(move || {
            println!("[Image {}] [CPU] Classifying...", image_id);
            
            // Simulate CPU-based classification
            std::thread::sleep(std::time::Duration::from_millis(30));
            
            let class = image_id % 3;
            let classes = ["Cat", "Dog", "Bird"];
            println!("[Image {}] [CPU] Classified as: {}", image_id, classes[class]);
        });
        
        // Build pipeline: load → preprocess → extract → classify
        load.precede(&preprocess);
        preprocess.precede(&extract);
        extract.precede(&classify);
        
        // Execute this image's pipeline
        executor.run(&taskflow).wait();
        println!();
    }
    
    let total_elapsed = total_start.elapsed();
    let throughput = num_images as f64 / total_elapsed.as_secs_f64();
    
    println!("═══════════════════════════════════════");
    println!("Pipeline complete!");
    println!("Processed {} images in {:?}", num_images, total_elapsed);
    println!("Throughput: {:.2} images/sec", throughput);
    println!("Average: {:.2} ms/image", total_elapsed.as_millis() as f64 / num_images as f64);
    println!("═══════════════════════════════════════");
}
