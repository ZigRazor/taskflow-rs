use std::hint::black_box;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use taskflow_rs::{parallel_for_each, parallel_reduce, parallel_transform, Executor, Taskflow};

fn main() {
    println!("=== Parallel Algorithms Benchmark ===\n");
    println!("Comparing parallel vs sequential performance\n");

    benchmark_for_each();
    println!();

    benchmark_reduce();
    println!();

    benchmark_transform();
}

fn heavy_work(x: i32) -> i32 {
    // Simulate CPU-intensive work
    let mut result = x;
    for _ in 0..1000 {
        result = result.wrapping_mul(1664525).wrapping_add(1013904223);
        result ^= result >> 13;
    }
    black_box(result) // Prevent optimization
}

fn benchmark_for_each() {
    println!("1. For-Each Benchmark (10000 items with heavy work)");

    let data: Vec<i32> = (0..10000).collect();

    // Sequential
    let counter_seq = Arc::new(Mutex::new(0));
    let counter_clone = Arc::clone(&counter_seq);
    let start = Instant::now();

    for item in data.iter() {
        let result = heavy_work(*item);
        black_box(result); // Prevent optimization
        *counter_clone.lock().unwrap() += 1;
    }

    let seq_duration = start.elapsed();
    println!("   Sequential: {:?}", seq_duration);

    // Parallel with different worker counts
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();

        let counter = Arc::new(Mutex::new(0));
        let data_copy = data.clone();

        let counter_clone = Arc::clone(&counter);
        let start = Instant::now();

        parallel_for_each(&mut taskflow, data_copy, 2500, move |item| {
            let result = heavy_work(item);
            black_box(result);
            *counter_clone.lock().unwrap() += 1;
        });

        executor.run(&taskflow).wait();
        let duration = start.elapsed();

        let speedup = seq_duration.as_secs_f64() / duration.as_secs_f64();
        println!(
            "   {} workers: {:?} (speedup: {:.2}x)",
            num_workers, duration, speedup
        );
    }
}

fn benchmark_reduce() {
    println!("2. Reduce Benchmark (sum of 100000 items with work)");

    let data: Vec<i32> = (0..100000).collect();

    // Sequential
    let start = Instant::now();
    let seq_sum: i32 = data.iter().map(|&x| heavy_work(x)).sum();
    black_box(seq_sum); // Prevent optimization
    let seq_duration = start.elapsed();
    println!("   Sequential: {:?}", seq_duration);

    // Parallel with different worker counts
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();

        let data_copy = data.clone();
        let start = Instant::now();

        let (_task, result) = parallel_reduce(&mut taskflow, data_copy, 25000, 0, |acc, item| {
            acc + heavy_work(item)
        });

        executor.run(&taskflow).wait();
        black_box(*result.lock().unwrap()); // Prevent optimization
        let duration = start.elapsed();

        let speedup = seq_duration.as_secs_f64() / duration.as_secs_f64();
        println!(
            "   {} workers: {:?} (speedup: {:.2}x)",
            num_workers, duration, speedup
        );
    }
}

fn benchmark_transform() {
    println!("3. Transform Benchmark (transform 50000 items with work)");

    let data: Vec<i32> = (0..50000).collect();

    // Sequential
    let start = Instant::now();
    let seq_result: Vec<i32> = data.iter().map(|&x| heavy_work(x)).collect();
    black_box(&seq_result); // Prevent optimization
    let seq_duration = start.elapsed();
    println!("   Sequential: {:?}", seq_duration);

    // Parallel with different worker counts
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();

        let data_copy = data.clone();
        let start = Instant::now();

        let (_tasks, results) = parallel_transform(&mut taskflow, data_copy, 12500, heavy_work);

        executor.run(&taskflow).wait();
        black_box(&*results.lock().unwrap()); // Prevent optimization
        let duration = start.elapsed();

        let speedup = seq_duration.as_secs_f64() / duration.as_secs_f64();
        println!(
            "   {} workers: {:?} (speedup: {:.2}x)",
            num_workers, duration, speedup
        );
    }
}
