use std::sync::{Arc, Mutex};
use std::time::Instant;
use taskflow_rs::{Executor, Taskflow};

fn main() {
    println!("Debug Benchmark - Testing if executor works at all\n");

    // Test with very simple tasks first
    println!("Test 1: 10 simple tasks, 1 worker");
    {
        let mut executor = Executor::new(1);
        let mut taskflow = Taskflow::new();

        let counter = Arc::new(Mutex::new(0));

        for i in 0..10 {
            let counter = Arc::clone(&counter);
            taskflow.emplace(move || {
                println!("  Task {} executing", i);
                *counter.lock().unwrap() += 1;
            });
        }

        println!("Created {} tasks", taskflow.size());
        println!("Starting execution...");

        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();

        println!("Completed in {:?}", duration);
        println!("Counter value: {}", *counter.lock().unwrap());
        println!("✓ Test 1 passed\n");
    }

    // Test with 2 workers
    println!("Test 2: 10 simple tasks, 2 workers");
    {
        let mut executor = Executor::new(2);
        let mut taskflow = Taskflow::new();

        let counter = Arc::new(Mutex::new(0));

        for i in 0..10 {
            let counter = Arc::clone(&counter);
            taskflow.emplace(move || {
                println!("  Task {} executing", i);
                *counter.lock().unwrap() += 1;
            });
        }

        println!("Created {} tasks", taskflow.size());
        println!("Starting execution...");

        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();

        println!("Completed in {:?}", duration);
        println!("Counter value: {}", *counter.lock().unwrap());
        println!("✓ Test 2 passed\n");
    }

    // Test with actual work
    println!("Test 3: 10 tasks with actual work, 2 workers");
    {
        let mut executor = Executor::new(2);
        let mut taskflow = Taskflow::new();

        let counter = Arc::new(Mutex::new(0u64));

        for i in 0..10 {
            let counter = Arc::clone(&counter);
            taskflow.emplace(move || {
                // Do some work
                let mut sum = 0u64;
                for j in 0..100_000 {
                    sum = sum.wrapping_add((i * j) as u64);
                }
                *counter.lock().unwrap() += sum;
                println!("  Task {} done (result: {})", i, sum);
            });
        }

        println!("Created {} tasks", taskflow.size());
        println!("Starting execution...");

        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();

        println!("Completed in {:?}", duration);
        println!("✓ Test 3 passed\n");
    }

    println!("✅ All tests completed successfully!");
}
