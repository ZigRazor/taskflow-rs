use std::sync::{Arc, Mutex};
use std::time::Instant;
use taskflow_rs::{Executor, Taskflow};

fn heavy_computation(iterations: usize, seed: u64) -> u64 {
    let mut result = seed;
    for i in 0..iterations {
        result = result.wrapping_mul(1664525).wrapping_add(1013904223);
        result ^= result >> 13;
        result = result.wrapping_mul(2654435761);
        result ^= result >> 7;
        result = result.wrapping_add((i as u64).wrapping_mul(result | 1));
        result ^= result >> 17;
    }
    result
}

fn test_with_n_tasks(n: usize, iterations: usize) {
    println!(
        "\nTesting {} tasks with {} iterations each...",
        n, iterations
    );

    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    let counter = Arc::new(Mutex::new(0u64));

    for i in 0..n {
        let counter = Arc::clone(&counter);
        taskflow.emplace(move || {
            let result = heavy_computation(iterations, i as u64);
            *counter.lock().unwrap() += result;
        });
    }

    println!("  Created {} tasks, starting execution...", taskflow.size());

    let start = Instant::now();
    executor.run(&taskflow).wait();
    let duration = start.elapsed();

    println!("  ✓ Completed in {:?}", duration);
}

fn main() {
    println!("=== Incremental Scale Test ===\n");

    // Test with increasing number of tasks
    test_with_n_tasks(10, 10_000);
    test_with_n_tasks(20, 10_000);
    test_with_n_tasks(30, 10_000);
    test_with_n_tasks(50, 10_000);
    test_with_n_tasks(75, 10_000);
    test_with_n_tasks(100, 10_000);

    println!("\n✅ All scale tests passed!");

    // If we get here, try with heavier computation
    println!("\n=== Testing with heavier computation ===");
    test_with_n_tasks(10, 100_000);
    test_with_n_tasks(20, 100_000);
    test_with_n_tasks(50, 100_000);
    test_with_n_tasks(100, 100_000);

    println!("\n✅ All heavy tests passed!");
}
