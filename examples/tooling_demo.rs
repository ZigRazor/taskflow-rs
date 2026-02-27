// Tooling Demo - Comprehensive testing of profiling, monitoring, and visualization
#![allow(unused_imports)]

use taskflow_rs::{
    Executor, Taskflow,
};

// Import tooling separately to isolate any issues
use taskflow_rs::Profiler;
use taskflow_rs::PerformanceMetrics;
use taskflow_rs::DebugLogger;
use taskflow_rs::LogLevel;
use taskflow_rs::generate_dot_graph;
use taskflow_rs::generate_html_report;

use std::thread;
use std::time::Duration;

fn main() {
    println!("=== TaskFlow Tooling Demo ===\n");
    
    demo_profiling();
    println!();
    
    demo_performance_monitoring();
    println!();
    
    demo_debug_logging();
    println!();
    
    demo_visualization();
}

/// Demo 1: Profiling
fn demo_profiling() {
    println!("1. Profiling");
    println!("   Collecting execution statistics\n");
    
    let mut executor = Executor::new(4);
    let profiler = Profiler::new(4);
    
    // Enable profiling
    profiler.enable();
    profiler.start_run();
    
    // Create and run taskflow
    let mut taskflow = Taskflow::new();
    
    let task_a = taskflow.emplace(|| {
        thread::sleep(Duration::from_millis(100));
    }).name("Task A");
    
    let task_b = taskflow.emplace(|| {
        thread::sleep(Duration::from_millis(50));
    }).name("Task B");
    
    let task_c = taskflow.emplace(|| {
        thread::sleep(Duration::from_millis(75));
    }).name("Task C");
    
    task_a.precede(&task_b);
    task_a.precede(&task_c);
    
    // Record execution (simplified - normally integrated into executor)
    let start = std::time::Instant::now();
    executor.run(&taskflow).wait();
    let duration = start.elapsed();
    
    // Simulate recording
    profiler.record_task(1, Some("Task A".to_string()), start, Duration::from_millis(100), 0, 0);
    profiler.record_task(2, Some("Task B".to_string()), start, Duration::from_millis(50), 1, 1);
    profiler.record_task(3, Some("Task C".to_string()), start, Duration::from_millis(75), 2, 1);
    
    // Get profile
    if let Some(profile) = profiler.get_profile() {
        println!("{}", profile.summary());
        
        println!("   Longest task: {:?}", profile.longest_task().map(|t| &t.name));
        println!("   Shortest task: {:?}", profile.shortest_task().map(|t| &t.name));
    }
    
    println!("   ✓ Profiling complete");
}

/// Demo 2: Performance Monitoring
fn demo_performance_monitoring() {
    println!("2. Performance Monitoring");
    println!("   Real-time performance metrics\n");
    
    let metrics = PerformanceMetrics::new(4);
    metrics.start();
    
    // Simulate task execution
    for _ in 0..100 {
        metrics.record_task_completion(Duration::from_micros(500));
    }
    
    // Simulate some steals
    for _ in 0..10 {
        metrics.record_task_steal();
    }
    
    // Simulate worker activity
    metrics.record_worker_busy(0, Duration::from_millis(800));
    metrics.record_worker_idle(0, Duration::from_millis(200));
    
    metrics.record_worker_busy(1, Duration::from_millis(750));
    metrics.record_worker_idle(1, Duration::from_millis(250));
    
    println!("   Tasks completed: {}", metrics.tasks_completed());
    println!("   Tasks stolen: {} ({:.2}%)", 
             metrics.tasks_stolen(), metrics.steal_rate());
    println!("   Average task duration: {:?}", metrics.average_task_duration());
    println!("   Worker 0 utilization: {:.2}%", metrics.worker_utilization(0));
    println!("   Worker 1 utilization: {:.2}%", metrics.worker_utilization(1));
    println!("   Average utilization: {:.2}%", metrics.average_worker_utilization());
    
    println!("\n   ✓ Performance monitoring complete");
}

/// Demo 3: Debug Logging
fn demo_debug_logging() {
    println!("3. Debug Logging");
    println!("   Structured logging with levels\n");
    
    let logger = DebugLogger::new();
    logger.enable();
    logger.set_log_level(LogLevel::Debug);
    
    // Log some events
    logger.info("Executor", "Starting execution");
    logger.debug("Worker-0", "Popped task from queue");
    logger.log(LogLevel::Debug, "Task-1", "Executing task", Some(0), Some(1));
    logger.log(LogLevel::Debug, "Task-1", "Task completed", Some(0), Some(1));
    logger.warn("Scheduler", "Work stealing triggered");
    logger.debug("Worker-1", "Stole task from worker 0");
    logger.info("Executor", "All tasks completed");
    
    println!();
    let log_count = logger.get_logs().len();
    println!("   Logged {} events", log_count);
    
    // Save to file
    if let Err(e) = logger.save_to_file("execution.log") {
        println!("   Failed to save log: {}", e);
    } else {
        println!("   ✓ Logs saved to execution.log");
    }
}

/// Demo 4: Visualization
fn demo_visualization() {
    println!("4. Visualization");
    println!("   Generating graphs and reports\n");
    
    // Generate DOT graph
    let tasks = vec![
        (1, "Load Data".to_string()),
        (2, "Transform".to_string()),
        (3, "Compute A".to_string()),
        (4, "Compute B".to_string()),
        (5, "Merge Results".to_string()),
    ];
    
    let dependencies = vec![
        (1, 2),
        (2, 3),
        (2, 4),
        (3, 5),
        (4, 5),
    ];
    
    let dot = generate_dot_graph(&tasks, &dependencies);
    
    // Save DOT file
    if let Err(e) = std::fs::write("taskflow.dot", &dot) {
        println!("   Failed to save DOT graph: {}", e);
    } else {
        println!("   ✓ DOT graph saved to taskflow.dot");
        println!("     Generate PNG: dot -Tpng taskflow.dot -o taskflow.png");
    }
    
    // Generate HTML report
    let profiler = Profiler::new(4);
    profiler.enable();
    profiler.start_run();
    
    let start = std::time::Instant::now();
    profiler.record_task(1, Some("Load Data".to_string()), start, Duration::from_millis(100), 0, 0);
    profiler.record_task(2, Some("Transform".to_string()), start, Duration::from_millis(150), 0, 1);
    profiler.record_task(3, Some("Compute A".to_string()), start, Duration::from_millis(200), 1, 1);
    profiler.record_task(4, Some("Compute B".to_string()), start, Duration::from_millis(180), 2, 1);
    profiler.record_task(5, Some("Merge Results".to_string()), start, Duration::from_millis(50), 0, 2);
    
    if let Some(profile) = profiler.get_profile() {
        let html = generate_html_report(&profile);
        
        if let Err(e) = std::fs::write("report.html", &html) {
            println!("   Failed to save HTML report: {}", e);
        } else {
            println!("   ✓ HTML report saved to report.html");
            println!("     Open report.html in a browser");
        }
    }
    
    println!("\n   ✓ Visualization complete");
}
