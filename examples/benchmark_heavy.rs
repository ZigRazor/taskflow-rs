use taskflow_rs::{Executor, Taskflow};
use std::time::Instant;
use std::sync::{Arc, Mutex};

fn main() {
    println!("=== TaskFlow-RS Work-Stealing Benchmark ===\n");
    println!("NOTE: Run with --release for accurate results!\n");
    
    benchmark_cpu_intensive_parallel();
    println!();
    
    benchmark_fibonacci();
    println!();
    
    benchmark_matrix_ops();
}

/// Heavy CPU work to make parallelism worthwhile
fn heavy_computation(iterations: usize, seed: u64) -> u64 {
    let mut result = seed;
    for i in 0..iterations {
        // Complex arithmetic to prevent compiler optimization
        result = result.wrapping_mul(1664525).wrapping_add(1013904223);
        result ^= result >> 13;
        result = result.wrapping_mul(2654435761);
        result ^= result >> 7;
        result = result.wrapping_add((i as u64).wrapping_mul(result | 1));
        result ^= result >> 17;
    }
    result
}

/// Benchmark: CPU-intensive independent tasks
fn benchmark_cpu_intensive_parallel() {
    println!("1. CPU-Intensive Parallel Tasks");
    println!("   (100 tasks, each with 1M operations)");
    println!();
    
    let mut baseline_time = None;
    
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();
        
        let counter = Arc::new(Mutex::new(0u64));
        
        // Create 100 independent CPU-intensive tasks
        for i in 0..100 {
            let counter = Arc::clone(&counter);
            taskflow.emplace(move || {
                let result = heavy_computation(1_000_000, i as u64);
                *counter.lock().unwrap() += result;
            }).name(&format!("task_{}", i));
        }
        
        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();
        
        if baseline_time.is_none() {
            baseline_time = Some(duration.as_secs_f64());
        }
        
        let speedup = baseline_time.unwrap() / duration.as_secs_f64();
        let efficiency = (speedup / num_workers as f64) * 100.0;
        
        println!("  {} worker(s): {:>8.2?}  (speedup: {:.2}x, efficiency: {:.1}%)", 
                 num_workers, duration, speedup, efficiency);
    }
}

/// Recursive fibonacci (intentionally inefficient for CPU work)
fn fibonacci(n: u32) -> u64 {
    if n <= 1 {
        return n as u64;
    }
    fibonacci(n - 1).wrapping_add(fibonacci(n - 2))
}

/// Benchmark: Parallel fibonacci computation
fn benchmark_fibonacci() {
    println!("2. Parallel Fibonacci Computation");
    println!("   (32 tasks computing fib(30))");
    println!();
    
    let mut baseline_time = None;
    
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();
        
        let results = Arc::new(Mutex::new(0u64));
        
        // Create 32 tasks computing fibonacci(30)
        for _i in 0..32 {
            let results = Arc::clone(&results);
            taskflow.emplace(move || {
                let fib_result = fibonacci(30);
                *results.lock().unwrap() += fib_result;
            });
        }
        
        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();
        
        if baseline_time.is_none() {
            baseline_time = Some(duration.as_secs_f64());
        }
        
        let speedup = baseline_time.unwrap() / duration.as_secs_f64();
        let efficiency = (speedup / num_workers as f64) * 100.0;
        
        println!("  {} worker(s): {:>8.2?}  (speedup: {:.2}x, efficiency: {:.1}%)", 
                 num_workers, duration, speedup, efficiency);
    }
}

/// Matrix multiplication for CPU work
fn matrix_multiply(size: usize, seed: usize) -> f64 {
    let mut a = vec![vec![0.0; size]; size];
    let mut b = vec![vec![0.0; size]; size];
    let mut c = vec![vec![0.0; size]; size];
    
    // Initialize matrices
    for i in 0..size {
        for j in 0..size {
            a[i][j] = ((i + j + seed) % 100) as f64;
            b[i][j] = ((i * j + seed) % 100) as f64;
        }
    }
    
    // Multiply
    for i in 0..size {
        for j in 0..size {
            for k in 0..size {
                c[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    
    // Return sum of diagonal
    (0..size).map(|i| c[i][i]).sum()
}

/// Benchmark: Parallel matrix multiplication
fn benchmark_matrix_ops() {
    println!("3. Parallel Matrix Operations");
    println!("   (64 tasks, 80x80 matrix multiplication each)");
    println!();
    
    let mut baseline_time = None;
    
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();
        
        let counter = Arc::new(Mutex::new(0.0));
        
        // Create 64 tasks doing matrix multiplication
        for i in 0..64 {
            let counter = Arc::clone(&counter);
            taskflow.emplace(move || {
                let result = matrix_multiply(80, i);
                *counter.lock().unwrap() += result;
            });
        }
        
        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();
        
        if baseline_time.is_none() {
            baseline_time = Some(duration.as_secs_f64());
        }
        
        let speedup = baseline_time.unwrap() / duration.as_secs_f64();
        let efficiency = (speedup / num_workers as f64) * 100.0;
        
        println!("  {} worker(s): {:>8.2?}  (speedup: {:.2}x, efficiency: {:.1}%)", 
                 num_workers, duration, speedup, efficiency);
    }
}
