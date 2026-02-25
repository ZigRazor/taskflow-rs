use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::thread;

fn main() {
    println!("=== Run Variants Demo ===\n");
    
    demo_run_n();
    println!();
    
    demo_run_until();
    println!();
    
    demo_run_many();
    println!();
    
    demo_run_many_concurrent();
}

/// Demo 1: Run N times - execute a workflow multiple times
fn demo_run_n() {
    println!("1. Run N Times");
    println!("   Executing workflow 5 times\n");
    
    let mut executor = Executor::new(4);
    let counter = Arc::new(AtomicUsize::new(0));
    
    let c = counter.clone();
    let start = Instant::now();
    
    executor.run_n(5, move || {
        let mut taskflow = Taskflow::new();
        let c = c.clone();
        
        taskflow.emplace(move || {
            let count = c.fetch_add(1, Ordering::Relaxed) + 1;
            println!("   [Iteration {}] Processing...", count);
            thread::sleep(Duration::from_millis(100));
        });
        
        taskflow
    }).wait();
    
    let elapsed = start.elapsed();
    
    println!("\n   ✓ Completed {} iterations in {:?}", 
             counter.load(Ordering::Relaxed), elapsed);
}

/// Demo 2: Run until condition - execute until predicate is true
fn demo_run_until() {
    println!("2. Run Until Condition");
    println!("   Running until sum reaches 50\n");
    
    let mut executor = Executor::new(4);
    let sum = Arc::new(AtomicUsize::new(0));
    let iteration = Arc::new(AtomicUsize::new(0));
    
    let s = sum.clone();
    let i = iteration.clone();
    let start = Instant::now();
    
    let s_check = sum.clone();
    executor.run_until(
        move || {
            let mut taskflow = Taskflow::new();
            let s = s.clone();
            let i = i.clone();
            
            taskflow.emplace(move || {
                let iter = i.fetch_add(1, Ordering::Relaxed);
                let value = (iter % 10) + 1; // Cycles through 1-10
                let new_sum = s.fetch_add(value, Ordering::Relaxed) + value;
                println!("   [Iteration {}] Added {} (total: {})", iter + 1, value, new_sum);
                thread::sleep(Duration::from_millis(50));
            });
            
            taskflow
        },
        move || s_check.load(Ordering::Relaxed) >= 50
    ).wait();
    
    let elapsed = start.elapsed();
    let final_sum = sum.load(Ordering::Relaxed);
    
    println!("\n   ✓ Stopped at sum = {} in {:?}", final_sum, elapsed);
}

/// Demo 3: Run many workflows concurrently
fn demo_run_many() {
    println!("3. Run Many Workflows");
    println!("   Running 3 different workflows concurrently\n");
    
    let mut executor = Executor::new(4);
    
    // Create three different workflows
    let flow1 = create_workflow("Flow1", 3);
    let flow2 = create_workflow("Flow2", 2);
    let flow3 = create_workflow("Flow3", 4);
    
    let start = Instant::now();
    
    // Run all concurrently
    let futures = executor.run_many(&[&flow1, &flow2, &flow3]);
    
    println!("   All workflows started, waiting for completion...\n");
    
    // Wait for all to complete
    for (i, future) in futures.into_iter().enumerate() {
        future.wait();
        println!("   Workflow {} completed", i + 1);
    }
    
    let elapsed = start.elapsed();
    println!("\n   ✓ All workflows completed in {:?}", elapsed);
}

/// Demo 4: Run many with convenience method
fn demo_run_many_concurrent() {
    println!("4. Run Many (Convenience Method)");
    println!("   Running multiple workflows and waiting\n");
    
    let mut executor = Executor::new(4);
    
    let flows: Vec<_> = (0..5)
        .map(|i| create_workflow(&format!("Pipeline{}", i), 2))
        .collect();
    
    let refs: Vec<_> = flows.iter().collect();
    
    let start = Instant::now();
    
    println!("   Running 5 pipelines concurrently...\n");
    executor.run_many_and_wait(&refs);
    
    let elapsed = start.elapsed();
    println!("\n   ✓ All pipelines completed in {:?}", elapsed);
}

// Helper function to create a workflow
fn create_workflow(name: &str, num_tasks: usize) -> Taskflow {
    let mut taskflow = Taskflow::new();
    let name = name.to_string();
    
    for i in 0..num_tasks {
        let n = name.clone();
        taskflow.emplace(move || {
            println!("   [{}] Task {} executing", n, i);
            thread::sleep(Duration::from_millis(50 + (i as u64 * 20)));
        }).name(&format!("{}_{}", name, i));
    }
    
    taskflow
}
