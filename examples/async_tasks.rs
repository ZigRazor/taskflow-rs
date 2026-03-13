// This example requires the 'async' feature
// Run with: cargo run --features async --example async_tasks

use std::sync::{Arc, Mutex};
use std::time::Duration;
use taskflow_rs::{AsyncExecutor, Taskflow};

#[tokio::main]
async fn main() {
    println!("=== Async Tasks Demo ===\n");

    demo_basic_async().await;
    println!();

    demo_mixed_sync_async().await;
    println!();

    demo_async_http_requests().await;
    println!();

    demo_async_pipeline().await;
}

async fn demo_basic_async() {
    println!("1. Basic Async Tasks");
    println!("   Running async tasks with tokio\n");

    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();

    // Create async tasks
    let task_a = taskflow
        .emplace_async(|| async {
            println!("   [Task A] Starting async work...");
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("   [Task A] Completed!");
        })
        .name("task_a");

    let task_b = taskflow
        .emplace_async(|| async {
            println!("   [Task B] Starting async work...");
            tokio::time::sleep(Duration::from_millis(50)).await;
            println!("   [Task B] Completed!");
        })
        .name("task_b");

    let task_c = taskflow
        .emplace_async(|| async {
            println!("   [Task C] Starting async work...");
            tokio::time::sleep(Duration::from_millis(75)).await;
            println!("   [Task C] Completed!");
        })
        .name("task_c");

    // Set dependencies
    task_a.precede(&task_b);
    task_a.precede(&task_c);

    executor.run_async(&taskflow).await;
    println!("   ✓ All async tasks completed");
}

async fn demo_mixed_sync_async() {
    println!("2. Mixed Sync and Async Tasks");
    println!("   Combining synchronous and asynchronous tasks\n");

    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();

    let counter = Arc::new(Mutex::new(0));

    // Sync task
    let counter_clone = Arc::clone(&counter);
    let sync_task = taskflow
        .emplace(move || {
            println!("   [Sync] Processing synchronously");
            *counter_clone.lock().unwrap() += 1;
            println!("   [Sync] Counter = {}", counter_clone.lock().unwrap());
        })
        .name("sync_task");

    // Async task that depends on sync task
    let counter_clone = Arc::clone(&counter);
    let async_task = taskflow
        .emplace_async(move || async move {
            println!("   [Async] Starting async work");
            tokio::time::sleep(Duration::from_millis(50)).await;
            *counter_clone.lock().unwrap() += 10;
            println!("   [Async] Counter = {}", counter_clone.lock().unwrap());
        })
        .name("async_task");

    // Another sync task that depends on async task
    let counter_clone = Arc::clone(&counter);
    let final_sync = taskflow
        .emplace(move || {
            println!(
                "   [Final] Final counter = {}",
                counter_clone.lock().unwrap()
            );
        })
        .name("final");

    sync_task.precede(&async_task);
    async_task.precede(&final_sync);

    executor.run_async(&taskflow).await;
    println!("   ✓ Mixed workflow completed");
}

async fn demo_async_http_requests() {
    println!("3. Simulated Async HTTP Requests");
    println!("   Parallel async I/O operations\n");

    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();

    let results = Arc::new(Mutex::new(Vec::new()));

    // Simulate multiple API calls
    for i in 0..5 {
        let results = Arc::clone(&results);
        taskflow
            .emplace_async(move || async move {
                println!("   [Request {}] Fetching data...", i);

                // Simulate network delay
                tokio::time::sleep(Duration::from_millis(50 + i * 10)).await;

                let data = format!("data_{}", i);
                results.lock().unwrap().push(data.clone());

                println!("   [Request {}] Received: {}", i, data);
            })
            .name(&format!("request_{}", i));
    }

    executor.run_async(&taskflow).await;

    let results = results.lock().unwrap();
    println!("   ✓ Received {} responses: {:?}", results.len(), *results);
}

async fn demo_async_pipeline() {
    println!("4. Async Data Pipeline");
    println!("   Multi-stage async processing\n");

    let executor = AsyncExecutor::new(4);
    let mut taskflow = Taskflow::new();

    let data = Arc::new(Mutex::new(Vec::new()));

    // Stage 1: Fetch data
    let data_clone = Arc::clone(&data);
    let fetch = taskflow
        .emplace_async(move || async move {
            println!("   [Fetch] Fetching raw data...");
            tokio::time::sleep(Duration::from_millis(100)).await;
            data_clone
                .lock()
                .unwrap()
                .extend_from_slice(&[1, 2, 3, 4, 5]);
            println!(
                "   [Fetch] Fetched {} items",
                data_clone.lock().unwrap().len()
            );
        })
        .name("fetch");

    // Stage 2: Transform data
    let data_clone = Arc::clone(&data);
    let transform = taskflow
        .emplace_async(move || async move {
            println!("   [Transform] Transforming data...");
            tokio::time::sleep(Duration::from_millis(50)).await;
            let mut d = data_clone.lock().unwrap();
            for item in d.iter_mut() {
                *item *= 2;
            }
            println!("   [Transform] Transformed {} items", d.len());
        })
        .name("transform");

    // Stage 3: Filter data
    let data_clone = Arc::clone(&data);
    let filter = taskflow
        .emplace_async(move || async move {
            println!("   [Filter] Filtering data...");
            tokio::time::sleep(Duration::from_millis(30)).await;
            let mut d = data_clone.lock().unwrap();
            d.retain(|&x| x > 5);
            println!("   [Filter] Retained {} items", d.len());
        })
        .name("filter");

    // Stage 4: Save results
    let data_clone = Arc::clone(&data);
    let save = taskflow
        .emplace_async(move || async move {
            println!("   [Save] Saving results...");
            tokio::time::sleep(Duration::from_millis(40)).await;
            let d = data_clone.lock().unwrap();
            println!("   [Save] Final data: {:?}", *d);
        })
        .name("save");

    // Set up pipeline
    fetch.precede(&transform);
    transform.precede(&filter);
    filter.precede(&save);

    executor.run_async(&taskflow).await;
    println!("   ✓ Pipeline completed");
}
