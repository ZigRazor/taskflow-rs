use taskflow_rs::{Executor, Taskflow, Metrics};
use std::time::Duration;
use std::thread;

fn main() {
    println!("=== Built-in Metrics Demo ===\n");
    
    demo_basic_metrics();
    println!();
    
    demo_worker_metrics();
    println!();
    
    demo_performance_tracking();
    println!();
    
    demo_comprehensive_summary();
}

/// Demo 1: Basic Metrics
fn demo_basic_metrics() {
    println!("1. Basic Metrics");
    println!("   Track task execution and success rate\n");
    
    let metrics = Metrics::new(4);
    metrics.start();
    
    // Simulate task execution
    for i in 0..10 {
        metrics.record_task_start(i);
        thread::sleep(Duration::from_millis(5));
        metrics.record_task_completion(i, i % 4);
    }
    
    // Add some failures
    metrics.record_task_start(100);
    metrics.record_task_failure(100);
    
    metrics.record_task_start(101);
    metrics.record_task_failure(101);
    
    println!("   Total Tasks Executed: {}", metrics.total_tasks_executed());
    println!("   Total Tasks Failed: {}", metrics.total_tasks_failed());
    println!("   Success Rate: {:.2}%", metrics.success_rate() * 100.0);
    println!("   Average Task Duration: {:?}", metrics.average_task_duration());
    println!("   Tasks/Second: {:.2}", metrics.tasks_per_second());
    
    assert_eq!(metrics.total_tasks_executed(), 10);
    assert_eq!(metrics.total_tasks_failed(), 2);
    println!("\n   ✓ Basic metrics tracking works");
}

/// Demo 2: Worker Metrics
fn demo_worker_metrics() {
    println!("2. Worker Metrics");
    println!("   Track individual worker performance\n");
    
    let metrics = Metrics::new(4);
    metrics.start();
    
    // Simulate uneven task distribution
    for i in 0..20 {
        let worker_id = i % 4;
        metrics.record_task_start(i);
        
        // Worker 0 is slower
        let delay = if worker_id == 0 { 10 } else { 5 };
        thread::sleep(Duration::from_millis(delay));
        
        metrics.record_task_completion(i, worker_id);
    }
    
    println!("   Worker Statistics:");
    for worker_id in 0..4 {
        let tasks = metrics.worker_task_count(worker_id);
        let utilization = metrics.worker_utilization(worker_id);
        println!("     Worker {}: {} tasks, {:.2}% utilization", 
                 worker_id, tasks, utilization * 100.0);
    }
    
    println!("\n   Average Worker Utilization: {:.2}%", 
             metrics.average_worker_utilization() * 100.0);
    
    println!("\n   ✓ Worker metrics tracking works");
}

/// Demo 3: Performance Tracking
fn demo_performance_tracking() {
    println!("3. Performance Tracking");
    println!("   Monitor performance over time\n");
    
    let mut executor = Executor::new(4);
    let metrics = Metrics::new(4);
    metrics.start();
    
    // Run multiple batches and track performance
    for batch in 0..3 {
        let mut taskflow = Taskflow::new();
        
        println!("   Batch {}: Running 10 tasks", batch + 1);
        
        for i in 0..10 {
            let task_id = batch * 10 + i;
            taskflow.emplace(move || {
                thread::sleep(Duration::from_millis(10));
            }).name(&format!("task_{}", task_id));
        }
        
        let start = std::time::Instant::now();
        executor.run(&taskflow).wait();
        let elapsed = start.elapsed();
        
        // Record metrics for this batch
        for i in 0..10 {
            let task_id = batch * 10 + i;
            metrics.record_task_start(task_id);
            metrics.record_task_completion(task_id, (i % 4) as usize);
        }
        
        println!("     Completed in {:?}", elapsed);
        println!("     Current throughput: {:.2} tasks/sec", metrics.tasks_per_second());
    }
    
    println!("\n   Final Performance:");
    println!("     Total tasks: {}", metrics.total_tasks_executed());
    println!("     Overall throughput: {:.2} tasks/sec", metrics.tasks_per_second());
    
    println!("\n   ✓ Performance tracking works");
}

/// Demo 4: Comprehensive Summary
fn demo_comprehensive_summary() {
    println!("4. Comprehensive Summary");
    println!("   Get complete metrics overview\n");
    
    let metrics = Metrics::new(8);
    metrics.start();
    
    // Simulate a complex workload
    for i in 0..100 {
        metrics.record_task_start(i);
        
        let delay = match i % 10 {
            0..=2 => 5,  // Fast tasks
            3..=7 => 10, // Medium tasks
            _ => 20,     // Slow tasks
        };
        
        thread::sleep(Duration::from_millis(delay));
        metrics.record_task_completion(i, (i % 8) as usize);
    }
    
    // Add some failures
    for i in 100..105 {
        metrics.record_task_start(i);
        metrics.record_task_failure(i);
    }
    
    // Record memory usage
    metrics.record_memory_usage(50 * 1024 * 1024); // 50 MB
    thread::sleep(Duration::from_millis(10));
    metrics.record_memory_usage(75 * 1024 * 1024); // 75 MB (peak)
    thread::sleep(Duration::from_millis(10));
    metrics.record_memory_usage(60 * 1024 * 1024); // 60 MB
    
    // Get summary
    let summary = metrics.summary();
    println!("{}", summary);
    
    // Get task timing histogram
    let histogram = metrics.task_timing_histogram();
    println!("Task Duration Histogram:");
    for (duration, count) in histogram.iter().take(5) {
        println!("  {:?}: {} tasks", duration, count);
    }
    
    assert!(summary.total_tasks_executed > 0);
    assert!(summary.success_rate > 0.0);
    println!("\n   ✓ Comprehensive metrics work");
}
