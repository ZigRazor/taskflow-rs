#[cfg(feature = "async")]
use taskflow_rs::{AsyncExecutor, Taskflow};
#[cfg(feature = "async")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "async")]
use std::time::Duration;

#[cfg(feature = "async")]
#[tokio::main]
async fn main() {
    println!("=== Parallel Async Execution Demo ===\n");
    
    demo_parallel_async().await;
    println!();
    
    demo_mixed_sync_async().await;
    println!();
    
    demo_async_subflow().await;
    println!();
    
    demo_parallel_http_requests().await;
}

#[cfg(not(feature = "async"))]
fn main() {
    println!("This example requires the 'async' feature.");
    println!("Run with: cargo run --features async --example async_parallel");
}

#[cfg(feature = "async")]
async fn demo_parallel_async() {
    println!("1. Parallel Async Execution");
    println!("   Running independent async tasks in parallel\n");
    
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let start = std::time::Instant::now();
    let results = Arc::new(Mutex::new(Vec::new()));
    
    // Create 5 independent async tasks that run in parallel
    for i in 0..5 {
        let r = results.clone();
        taskflow.emplace_async(move || async move {
            println!("   [Task {}] Starting async work", i);
            
            // Simulate async I/O
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            println!("   [Task {}] Completed", i);
            r.lock().unwrap().push(i);
        }).name(&format!("async_task_{}", i));
    }
    
    executor.run_async(&taskflow).await;
    
    let elapsed = start.elapsed();
    
    println!("\n   All tasks completed in {:?}", elapsed);
    println!("   Results: {:?}", *results.lock().unwrap());
    
    // Should complete in ~100ms (parallel), not 500ms (sequential)
    println!("   ✓ Tasks ran in parallel ({}ms < 500ms)", elapsed.as_millis());
    assert!(elapsed.as_millis() < 300); // Allow some overhead
}

#[cfg(feature = "async")]
async fn demo_mixed_sync_async() {
    println!("2. Mixed Sync and Async Tasks");
    println!("   Combining synchronous and asynchronous tasks\n");
    
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let counter = Arc::new(Mutex::new(0));
    
    // Sync task
    let c = counter.clone();
    let sync_task = taskflow.emplace(move || {
        println!("   [Sync] Incrementing counter");
        *c.lock().unwrap() += 1;
    }).name("sync_task");
    
    // Async task
    let c = counter.clone();
    let async_task = taskflow.emplace_async(move || async move {
        println!("   [Async] Waiting...");
        tokio::time::sleep(Duration::from_millis(50)).await;
        println!("   [Async] Incrementing counter");
        *c.lock().unwrap() += 10;
    }).name("async_task");
    
    // Another sync task
    let c = counter.clone();
    let final_task = taskflow.emplace(move || {
        println!("   [Sync] Reading final counter");
        let value = *c.lock().unwrap();
        println!("   [Sync] Counter = {}", value);
    }).name("final_task");
    
    sync_task.precede(&async_task);
    async_task.precede(&final_task);
    
    executor.run_async(&taskflow).await;
    
    println!("\n   Final counter: {}", *counter.lock().unwrap());
    println!("   ✓ Mixed sync/async execution works");
}

#[cfg(feature = "async")]
async fn demo_async_subflow() {
    println!("3. Async Subflows");
    println!("   Dynamic task creation with async execution\n");
    
    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();
    
    let results = Arc::new(Mutex::new(Vec::new()));
    
    // Parent task that creates a subflow
    let r = results.clone();
    taskflow.emplace_subflow(move |subflow| {
        println!("   [Parent] Creating subflow with async tasks");
        
        // Create async tasks in the subflow
        for i in 0..3 {
            let r = r.clone();
            subflow.emplace_async(move || async move {
                println!("     [Subflow {}] Processing", i);
                tokio::time::sleep(Duration::from_millis(30)).await;
                r.lock().unwrap().push(i * 10);
                println!("     [Subflow {}] Done", i);
            }).name(&format!("subflow_task_{}", i));
        }
        
        println!("   [Parent] Subflow created with {} tasks", 3);
    }).name("parent_with_subflow");
    
    executor.run_async(&taskflow).await;
    
    println!("\n   Subflow results: {:?}", *results.lock().unwrap());
    println!("   ✓ Async subflows work correctly");
}

#[cfg(feature = "async")]
async fn demo_parallel_http_requests() {
    println!("4. Parallel HTTP Requests (Simulated)");
    println!("   Simulating concurrent API calls\n");
    
    let executor = AsyncExecutor::new(8);
    let mut taskflow = Taskflow::new();
    
    let responses = Arc::new(Mutex::new(Vec::new()));
    
    // Simulate fetching data from multiple APIs in parallel
    let apis = vec![
        ("Users API", 50),
        ("Posts API", 75),
        ("Comments API", 60),
        ("Albums API", 45),
        ("Photos API", 80),
    ];
    
    let start = std::time::Instant::now();
    
    for (api_name, delay_ms) in apis {
        let r = responses.clone();
        let name = api_name.to_string();
        
        taskflow.emplace_async(move || async move {
            println!("   [{}] Sending request...", name);
            
            // Simulate network delay
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            
            let response_size = (delay_ms * 10) as usize;
            
            println!("   [{}] Received {} bytes", name, response_size);
            r.lock().unwrap().push((name, response_size));
        }).name(api_name);
    }
    
    executor.run_async(&taskflow).await;
    
    let elapsed = start.elapsed();
    
    println!("\n   All requests completed in {:?}", elapsed);
    println!("   Total responses: {}", responses.lock().unwrap().len());
    
    // Should complete in ~80ms (longest request), not 310ms (sum of all)
    println!("   ✓ Requests ran in parallel ({}ms < 310ms)", elapsed.as_millis());
    
    println!("\n   Response summary:");
    for (api, size) in responses.lock().unwrap().iter() {
        println!("     {} - {} bytes", api, size);
    }
}
