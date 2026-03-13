use std::sync::Arc;
use std::thread;
use std::time::Duration;
use taskflow_rs::pipeline::{ConcurrentPipeline, Token};

fn main() {
    println!("=== Advanced Pipeline Processing ===\n");

    demo_data_processing_pipeline();
    println!();

    demo_parallel_stages();
}

/// Demo 1: Multi-stage data processing pipeline
fn demo_data_processing_pipeline() {
    println!("1. Multi-Stage Data Processing");
    println!("   Raw data → Parse → Transform → Validate → Output\n");

    // Create pipelines for each stage
    let stage1_in = ConcurrentPipeline::new(10, 100); // Raw input
    let stage1_out = ConcurrentPipeline::new(10, 100); // Parsed
    let stage2_out = ConcurrentPipeline::new(10, 100); // Transformed
    let stage3_out = ConcurrentPipeline::new(10, 100); // Validated

    // Clone for threads
    let s1_in = stage1_in.clone();
    let s1_out = stage1_out.clone();
    let s2_in = stage1_out.clone();
    let s2_out = stage2_out.clone();
    let s3_in = stage2_out.clone();
    let s3_out = stage3_out.clone();

    // Stage 1: Parse (serial)
    let stage1 = thread::spawn(move || {
        println!("   [Stage 1: Parse] Started");
        while !s1_in.is_stopped() {
            if let Some(token) = s1_in.try_pop() {
                // Parse: convert string to number
                let parsed = token.data; // Already a number in this demo
                println!("   [Stage 1] Parsed token #{}: {}", token.index, parsed);
                s1_out.push(parsed).ok();
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
        s1_out.stop();
        println!("   [Stage 1: Parse] Stopped");
    });

    // Stage 2: Transform (parallel processing)
    let stage2 = thread::spawn(move || {
        println!("   [Stage 2: Transform] Started");
        while !s2_in.is_stopped() {
            if let Some(token) = s2_in.try_pop() {
                // Transform: square the number
                let transformed = token.data * token.data;
                println!(
                    "   [Stage 2] Transformed token #{}: {} → {}",
                    token.index, token.data, transformed
                );
                s2_out.push(transformed).ok();
                thread::sleep(Duration::from_millis(20)); // Simulate heavy processing
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
        s2_out.stop();
        println!("   [Stage 2: Transform] Stopped");
    });

    // Stage 3: Validate (serial)
    let stage3 = thread::spawn(move || {
        println!("   [Stage 3: Validate] Started");
        while !s3_in.is_stopped() {
            if let Some(token) = s3_in.try_pop() {
                // Validate: check if within range
                let valid = token.data < 1000;
                println!(
                    "   [Stage 3] Validated token #{}: {} (valid: {})",
                    token.index, token.data, valid
                );
                if valid {
                    s3_out.push(token.data).ok();
                }
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
        s3_out.stop();
        println!("   [Stage 3: Validate] Stopped");
    });

    // Producer: Generate input data
    let producer = thread::spawn(move || {
        println!("   [Producer] Started\n");
        for i in 1..=10 {
            stage1_in.push(i).unwrap();
            println!("   [Producer] Sent: {}", i);
            thread::sleep(Duration::from_millis(30));
        }
        stage1_in.stop();
        println!("\n   [Producer] Stopped");
    });

    // Consumer: Collect results
    let consumer = thread::spawn(move || {
        println!("   [Consumer] Started");
        let mut results = Vec::new();

        while !stage3_out.is_stopped() || stage3_out.tokens_in_flight() > 0 {
            if let Some(token) = stage3_out.try_pop() {
                println!("   [Consumer] Received final result: {}", token.data);
                results.push(token.data);
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }

        println!("   [Consumer] Final results: {:?}", results);
        println!("   [Consumer] Stopped");
    });

    // Wait for completion
    producer.join().unwrap();
    stage1.join().unwrap();
    stage2.join().unwrap();
    stage3.join().unwrap();
    consumer.join().unwrap();

    println!("\n   ✓ Multi-stage pipeline complete");
}

/// Demo 2: Parallel processing stages
fn demo_parallel_stages() {
    println!("2. Parallel Processing Stages");
    println!("   Multiple workers processing in parallel\n");

    let input = ConcurrentPipeline::new(20, 100);
    let output = ConcurrentPipeline::new(20, 100);

    // Spawn multiple parallel workers
    let num_workers = 3;
    let mut workers = Vec::new();

    for worker_id in 0..num_workers {
        let input_clone = input.clone();
        let output_clone = output.clone();

        let worker = thread::spawn(move || {
            println!("   [Worker {}] Started", worker_id);
            let mut processed = 0;

            while !input_clone.is_stopped() {
                if let Some(token) = input_clone.try_pop() {
                    // Simulate CPU-intensive work
                    thread::sleep(Duration::from_millis(50));

                    let result = token.data * 2;
                    println!(
                        "   [Worker {}] Processed token #{}: {} → {}",
                        worker_id, token.index, token.data, result
                    );

                    output_clone.push(result).ok();
                    processed += 1;
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }

            println!(
                "   [Worker {}] Stopped (processed {} tokens)",
                worker_id, processed
            );
        });

        workers.push(worker);
    }

    // Producer
    let input_clone = input.clone();
    let producer = thread::spawn(move || {
        for i in 1..=15 {
            input_clone.push(i * 10).unwrap();
            thread::sleep(Duration::from_millis(20));
        }
        input_clone.stop();
    });

    // Consumer
    let output_clone = output.clone();
    let consumer = thread::spawn(move || {
        let mut results = Vec::new();

        while !output_clone.is_stopped() || output_clone.tokens_in_flight() > 0 {
            if let Some(token) = output_clone.try_pop() {
                results.push(token.data);
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }

        println!("\n   Results: {:?}", results);
        println!("   Total processed: {}", results.len());
    });

    producer.join().unwrap();
    for worker in workers {
        worker.join().unwrap();
    }
    output.stop();
    consumer.join().unwrap();

    println!("\n   ✓ Parallel pipeline complete");
}
