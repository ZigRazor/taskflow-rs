use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use taskflow_rs::pipeline::ConcurrentPipeline;

fn main() {
    println!("=== Pipeline Benchmarks ===\n");

    benchmark_throughput();
    println!();

    benchmark_parallel_scaling();
    println!();

    benchmark_backpressure();
}

/// Benchmark 1: Pipeline throughput
fn benchmark_throughput() {
    println!("1. Throughput Benchmark");
    println!("   Measuring items/second through pipeline\n");

    let num_items = 10_000;
    let pipeline = ConcurrentPipeline::new(100, num_items);

    let producer_pipeline = pipeline.clone();
    let consumer_pipeline = pipeline.clone();
    let processed = Arc::new(AtomicUsize::new(0));
    let processed_clone = Arc::clone(&processed);

    let start = Instant::now();

    // Producer
    let producer = thread::spawn(move || {
        for i in 0..num_items {
            producer_pipeline.push(i).unwrap();
        }
        producer_pipeline.stop();
    });

    // Consumer
    let consumer = thread::spawn(move || {
        while !consumer_pipeline.is_stopped() || consumer_pipeline.tokens_in_flight() > 0 {
            if let Some(_token) = consumer_pipeline.try_pop() {
                // Simulate minimal processing
                processed_clone.fetch_add(1, Ordering::Relaxed);
            } else {
                thread::sleep(Duration::from_micros(1));
            }
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    let elapsed = start.elapsed();
    let throughput = num_items as f64 / elapsed.as_secs_f64();

    println!("   Processed: {} items", processed.load(Ordering::Relaxed));
    println!("   Time: {:?}", elapsed);
    println!("   Throughput: {:.2} items/sec", throughput);
}

/// Benchmark 2: Parallel stage scaling
fn benchmark_parallel_scaling() {
    println!("2. Parallel Scaling Benchmark");
    println!("   Testing throughput with different worker counts\n");

    let num_items = 5_000;
    let work_time_us = 100; // Microseconds of work per item

    for num_workers in [1, 2, 4, 8] {
        let input = ConcurrentPipeline::new(100, num_items);
        let output = ConcurrentPipeline::new(100, num_items);

        let start = Instant::now();

        // Spawn workers
        let mut workers = Vec::new();
        for _ in 0..num_workers {
            let i = input.clone();
            let o = output.clone();

            let worker = thread::spawn(move || {
                while !i.is_stopped() || i.tokens_in_flight() > 0 {
                    if let Some(token) = i.try_pop() {
                        // Simulate CPU work
                        thread::sleep(Duration::from_micros(work_time_us));
                        o.push(token.data).ok();
                    } else {
                        // No work available, sleep briefly
                        thread::sleep(Duration::from_micros(10));
                    }
                }
            });
            workers.push(worker);
        }

        // Producer
        let input_clone = input.clone();
        let producer = thread::spawn(move || {
            for i in 0..num_items {
                input_clone.push(i).unwrap();
            }
            input_clone.stop();
        });

        // Consumer
        let c = output.clone();
        let processed = Arc::new(AtomicUsize::new(0));
        let p = Arc::clone(&processed);
        let consumer = thread::spawn(move || {
            while !c.is_stopped() || c.tokens_in_flight() > 0 {
                if let Some(_token) = c.try_pop() {
                    p.fetch_add(1, Ordering::Relaxed);
                } else {
                    thread::sleep(Duration::from_micros(10));
                }
            }
        });

        producer.join().unwrap();
        for worker in workers {
            worker.join().unwrap();
        }
        output.stop(); // Stop output after all workers finish
        consumer.join().unwrap();

        let elapsed = start.elapsed();
        let throughput = num_items as f64 / elapsed.as_secs_f64();

        println!(
            "   {} worker(s): {:.2} items/sec ({:?})",
            num_workers, throughput, elapsed
        );
    }
}

/// Benchmark 3: Backpressure behavior
fn benchmark_backpressure() {
    println!("3. Backpressure Benchmark");
    println!("   Testing behavior under different buffer sizes\n");

    let num_items = 1_000;

    for buffer_size in [5, 20, 100] {
        let pipeline = ConcurrentPipeline::new(buffer_size, num_items);

        let p = pipeline.clone();
        let backpressure_hits = Arc::new(AtomicUsize::new(0));
        let bp_clone = Arc::clone(&backpressure_hits);

        let start = Instant::now();

        // Fast producer
        let producer = thread::spawn(move || {
            for i in 0..num_items {
                match p.push(i) {
                    Ok(_) => {}
                    Err(_) => {
                        bp_clone.fetch_add(1, Ordering::Relaxed);
                        // In real code, would retry
                        thread::sleep(Duration::from_micros(10));
                    }
                }
            }
            p.stop();
        });

        // Slow consumer
        let c = pipeline.clone();
        thread::sleep(Duration::from_millis(10)); // Let producer get ahead

        let consumer = thread::spawn(move || {
            while !c.is_stopped() || c.tokens_in_flight() > 0 {
                if let Some(_token) = c.try_pop() {
                    thread::sleep(Duration::from_micros(50)); // Slow processing
                } else {
                    thread::sleep(Duration::from_micros(10));
                }
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();

        let elapsed = start.elapsed();
        let bp_hits = backpressure_hits.load(Ordering::Relaxed);

        println!(
            "   Buffer size {}: {} backpressure events ({:?})",
            buffer_size, bp_hits, elapsed
        );
    }
}

/// Bonus: Memory pressure test
#[allow(dead_code)]
fn benchmark_memory_pressure() {
    println!("4. Memory Pressure");
    println!("   Testing with large token counts\n");

    let num_items = 100_000;
    let pipeline = ConcurrentPipeline::new(1000, num_items);

    let p = pipeline.clone();
    let start = Instant::now();

    // Producer
    let producer = thread::spawn(move || {
        for i in 0..num_items {
            p.push(vec![i; 100]).unwrap(); // Large data payload
        }
        p.stop();
    });

    // Consumer
    let c = pipeline.clone();
    let processed = Arc::new(AtomicUsize::new(0));
    let proc_clone = Arc::clone(&processed);

    let consumer = thread::spawn(move || {
        while !c.is_stopped() || c.tokens_in_flight() > 0 {
            if let Some(_token) = c.try_pop() {
                proc_clone.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    let elapsed = start.elapsed();

    println!(
        "   Processed: {} large items",
        processed.load(Ordering::Relaxed)
    );
    println!("   Time: {:?}", elapsed);
    println!(
        "   Avg: {:.2} µs/item",
        elapsed.as_micros() as f64 / num_items as f64
    );
}
