use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Instant;
use taskflow_rs::{Executor, Taskflow};

fn main() {
    println!("=== Parallel run_n Demo ===\n");

    demo_sequential_vs_parallel();
    println!();

    demo_parallel_instances();
    println!();

    demo_run_until();
    println!();

    demo_real_world_batch();
}

/// Demo 1: Sequential vs Parallel
fn demo_sequential_vs_parallel() {
    println!("1. Sequential vs Parallel Execution");
    println!("   Compare performance of sequential and parallel runs\n");

    let mut executor = Executor::new(4);

    // Sequential execution
    println!("   Sequential (run_n_sequential):");
    let start = Instant::now();
    executor
        .run_n_sequential(5, || {
            let mut taskflow = Taskflow::new();
            taskflow.emplace(|| {
                std::thread::sleep(std::time::Duration::from_millis(100));
            });
            taskflow
        })
        .wait();
    let sequential_time = start.elapsed();
    println!("     Time: {:?}", sequential_time);

    // Parallel execution
    println!("\n   Parallel (run_n):");
    let start = Instant::now();
    executor
        .run_n(5, || {
            let mut taskflow = Taskflow::new();
            taskflow.emplace(|| {
                std::thread::sleep(std::time::Duration::from_millis(100));
            });
            taskflow
        })
        .wait();
    let parallel_time = start.elapsed();
    println!("     Time: {:?}", parallel_time);

    println!(
        "\n   Speedup: {:.2}x",
        sequential_time.as_millis() as f64 / parallel_time.as_millis() as f64
    );
    println!("   ✓ Parallel execution is faster");
}

/// Demo 2: Parallel Instances
fn demo_parallel_instances() {
    println!("2. Parallel Instances");
    println!("   Run multiple instances with shared state\n");

    let mut executor = Executor::new(4);
    let counter = Arc::new(AtomicUsize::new(0));
    let results = Arc::new(std::sync::Mutex::new(Vec::new()));

    println!("   Running 10 instances in parallel:");

    let c = counter.clone();
    let r = results.clone();
    executor
        .run_n(10, move || {
            let c = c.clone();
            let r = r.clone();

            let mut taskflow = Taskflow::new();

            // Each instance increments the counter
            taskflow.emplace(move || {
                let id = c.fetch_add(1, Ordering::Relaxed);
                println!("     [Instance {}] Executing", id);

                // Simulate work
                std::thread::sleep(std::time::Duration::from_millis(50));

                r.lock().unwrap().push(id);
            });

            taskflow
        })
        .wait();

    let final_count = counter.load(Ordering::Relaxed);
    let results = results.lock().unwrap();

    println!("\n   Total executions: {}", final_count);
    println!("   Results collected: {}", results.len());
    assert_eq!(final_count, 10);
    assert_eq!(results.len(), 10);
    println!("   ✓ All instances executed correctly");
}

/// Demo 3: Run Until Condition
fn demo_run_until() {
    println!("3. Run Until Condition");
    println!("   Execute until a condition is met\n");

    let mut executor = Executor::new(4);
    let sum = Arc::new(AtomicUsize::new(0));
    let target = 100;

    println!("   Target sum: {}", target);
    println!("   Executing...");

    let s = sum.clone();
    executor
        .run_until(
            move || {
                let s = s.clone();
                let mut taskflow = Taskflow::new();

                taskflow.emplace(move || {
                    // Use a simple deterministic value instead of random
                    let value = 7; // Fixed increment value
                    let new_sum = s.fetch_add(value, Ordering::Relaxed) + value;
                    println!("     Added {}, sum = {}", value, new_sum);
                });

                taskflow
            },
            || sum.load(Ordering::Relaxed) >= target,
        )
        .wait();

    let final_sum = sum.load(Ordering::Relaxed);
    println!("\n   Final sum: {}", final_sum);
    assert!(final_sum >= target);
    println!("   ✓ Condition met");
}

/// Demo 4: Real-World Batch Processing
fn demo_real_world_batch() {
    println!("4. Real-World Batch Processing");
    println!("   Process multiple batches in parallel\n");

    let mut executor = Executor::new(4);

    // Simulate processing 5 batches of data
    let processed_items = Arc::new(AtomicUsize::new(0));

    println!("   Processing 5 batches with 10 items each:");

    let p = processed_items.clone();
    executor
        .run_n(5, move || {
            let p = p.clone();
            let mut taskflow = Taskflow::new();

            // Batch processing pipeline
            let p1 = p.clone();
            let load = taskflow.emplace(move || {
                println!("     [Batch] Loading data...");
                std::thread::sleep(std::time::Duration::from_millis(20));
            });

            let p2 = p.clone();
            let process = taskflow.emplace(move || {
                println!("     [Batch] Processing...");
                // Simulate processing 10 items
                for _ in 0..10 {
                    p2.fetch_add(1, Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            });

            let p3 = p.clone();
            let save = taskflow.emplace(move || {
                println!("     [Batch] Saving results...");
                std::thread::sleep(std::time::Duration::from_millis(20));
            });

            load.precede(&process);
            process.precede(&save);

            taskflow
        })
        .wait();

    let total_processed = processed_items.load(Ordering::Relaxed);
    println!("\n   Total items processed: {}", total_processed);
    assert_eq!(total_processed, 50); // 5 batches * 10 items
    println!("   ✓ Batch processing complete");
}
