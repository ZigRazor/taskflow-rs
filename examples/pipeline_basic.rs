use std::thread;
use std::time::Duration;
use taskflow_rs::pipeline::{ConcurrentPipeline, Token};

fn main() {
    println!("=== Pipeline Stream Processing Demo ===\n");

    demo_basic_pipeline();
    println!();

    demo_backpressure();
    println!();

    demo_producer_consumer();
}

/// Demo 1: Basic pipeline with token management
fn demo_basic_pipeline() {
    println!("1. Basic Pipeline");
    println!("   Simple data flow with tokens\n");

    let pipeline = ConcurrentPipeline::new(10, 100);

    // Push some data
    for i in 0..5 {
        pipeline.push(i * 10).unwrap();
        println!(
            "   Pushed: {}, Tokens in flight: {}",
            i * 10,
            pipeline.tokens_in_flight()
        );
    }

    println!();

    // Pop the data
    while let Some(token) = pipeline.try_pop() {
        println!("   Popped token #{}: data = {}", token.index, token.data);
    }

    println!("   ✓ Pipeline processed all tokens");
}

/// Demo 2: Backpressure handling
fn demo_backpressure() {
    println!("2. Backpressure Handling");
    println!("   Pipeline with limited buffer\n");

    // Small buffer to demonstrate backpressure
    let pipeline = ConcurrentPipeline::new(3, 10);

    // Try to push more than buffer size
    for i in 0..3 {
        match pipeline.push(i) {
            Ok(_) => println!("   ✓ Pushed {}", i),
            Err(e) => println!("   ✗ Failed to push {}: {}", i, e),
        }
    }

    println!(
        "   Buffer full (3/3), tokens in flight: {}",
        pipeline.tokens_in_flight()
    );

    // Pop one to make space
    if let Some(token) = pipeline.try_pop() {
        println!("   Popped token #{} to make space", token.index);
    }

    // Now we can push again
    match pipeline.push(99) {
        Ok(_) => println!("   ✓ Pushed 99 after making space"),
        Err(e) => println!("   ✗ Failed: {}", e),
    }

    println!("   ✓ Backpressure handled correctly");
}

/// Demo 3: Producer-Consumer pattern
fn demo_producer_consumer() {
    println!("3. Producer-Consumer Pattern");
    println!("   Multi-threaded stream processing\n");

    let pipeline = ConcurrentPipeline::new(10, 50);
    let pipeline_producer = pipeline.clone();
    let pipeline_consumer = pipeline.clone();

    // Producer thread
    let producer = thread::spawn(move || {
        for i in 0..20 {
            // Simulate work
            thread::sleep(Duration::from_millis(10));

            match pipeline_producer.push(i) {
                Ok(_) => println!("   [Producer] Sent: {}", i),
                Err(e) => {
                    println!("   [Producer] Backpressure: {}", e);
                    // In real code, would retry or wait
                    break;
                }
            }
        }
        println!("   [Producer] Finished");
    });

    // Consumer thread
    let consumer = thread::spawn(move || {
        let mut processed = 0;

        loop {
            // Simulate processing work
            thread::sleep(Duration::from_millis(15));

            if let Some(token) = pipeline_consumer.try_pop() {
                println!(
                    "   [Consumer] Processing token #{}: {}",
                    token.index, token.data
                );
                processed += 1;
            } else if processed >= 20 || pipeline_consumer.is_stopped() {
                break;
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }

        println!("   [Consumer] Processed {} tokens", processed);
    });

    producer.join().unwrap();
    thread::sleep(Duration::from_millis(500)); // Let consumer finish
    pipeline.stop();
    consumer.join().unwrap();

    println!("   ✓ Producer-consumer pipeline complete");
}
