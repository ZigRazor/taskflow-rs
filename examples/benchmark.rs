use taskflow_rs::{Executor, Taskflow};
use std::time::Instant;
use std::sync::{Arc, Mutex};

fn main() {
    println!("=== TaskFlow-RS Work-Stealing Benchmark ===\n");
    
    benchmark_wide_graph();
    println!();
    
    benchmark_deep_graph();
    println!();
    
    benchmark_parallel_workload();
}

/// Benchmark: Wide graph with many independent tasks
fn benchmark_wide_graph() {
    println!("1. Wide Graph Benchmark (1000 independent tasks)");
    
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();
        
        let counter = Arc::new(Mutex::new(0u64));
        let mut tasks = Vec::new();
        
        // Create 1000 independent tasks
        for i in 0..1000 {
            let counter = Arc::clone(&counter);
            let task = taskflow.emplace(move || {
                // Simulate some work
                let mut sum = 0u64;
                for j in 0..1000 {
                    sum = sum.wrapping_add((i * j) as u64);
                }
                *counter.lock().unwrap() += sum;
            }).name(&format!("task_{}", i));
            
            tasks.push(task);
        }
        
        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();
        
        println!("  {} workers: {:?}", num_workers, duration);
    }
}

/// Benchmark: Deep graph with sequential dependencies
fn benchmark_deep_graph() {
    println!("2. Deep Graph Benchmark (100 sequential stages, 10 tasks each)");
    
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();
        
        let counter = Arc::new(Mutex::new(0u64));
        let num_stages = 100;
        let tasks_per_stage = 10;
        
        let mut prev_stage = Vec::new();
        
        for stage in 0..num_stages {
            let mut current_stage = Vec::new();
            
            for task_idx in 0..tasks_per_stage {
                let counter = Arc::clone(&counter);
                let task = taskflow.emplace(move || {
                    let mut sum = 0u64;
                    for j in 0..100 {
                        sum = sum.wrapping_add((stage * task_idx * j) as u64);
                    }
                    *counter.lock().unwrap() += sum;
                }).name(&format!("stage_{}_task_{}", stage, task_idx));
                
                // Connect to all tasks in previous stage
                for prev_task in &prev_stage {
                    task.succeed(prev_task);
                }
                
                current_stage.push(task);
            }
            
            prev_stage = current_stage;
        }
        
        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();
        
        println!("  {} workers: {:?}", num_workers, duration);
    }
}

/// Benchmark: Mixed parallel workload
fn benchmark_parallel_workload() {
    println!("3. Mixed Parallel Workload (500 tasks, various dependencies)");
    
    for num_workers in [1, 2, 4, 8] {
        let mut executor = Executor::new(num_workers);
        let mut taskflow = Taskflow::new();
        
        let counter = Arc::new(Mutex::new(0u64));
        let num_tasks = 500;
        
        let mut tasks = Vec::new();
        
        // Create tasks with varying amounts of work
        for i in 0..num_tasks {
            let counter = Arc::clone(&counter);
            let work_amount = (i % 10 + 1) * 100;
            
            let task = taskflow.emplace(move || {
                let mut sum = 0u64;
                for j in 0..work_amount {
                    sum = sum.wrapping_add((i * j) as u64);
                }
                *counter.lock().unwrap() += sum;
            }).name(&format!("task_{}", i));
            
            tasks.push(task);
        }
        
        // Create semi-random dependencies
        for i in 0..num_tasks {
            // Each task depends on 0-3 previous tasks
            let deps = i % 4;
            for j in 0..deps {
                if i >= j + 1 {
                    tasks[i].succeed(&tasks[i - j - 1]);
                }
            }
        }
        
        let start = Instant::now();
        executor.run(&taskflow).wait();
        let duration = start.elapsed();
        
        println!("  {} workers: {:?}", num_workers, duration);
    }
}
