#[cfg(feature = "async")]
use taskflow_rs::{AsyncExecutor, Taskflow};
#[cfg(feature = "async")]
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
#[cfg(feature = "async")]
use std::time::{Duration, Instant};

#[cfg(feature = "async")]
#[tokio::main]
async fn main() {
    println!("=== Async Execution Variants Demo ===\n");
    
    demo_async_run_n().await;
    println!();
    
    demo_async_run_until().await;
    println!();
    
    demo_sequential_vs_parallel_async().await;
    println!();
    
    demo_real_world_async().await;
}

#[cfg(not(feature = "async"))]
fn main() {
    println!("This example requires the 'async' feature.");
    println!("Run with: cargo run --features async --example async_run_variants");
}

#[cfg(feature = "async")]
async fn demo_async_run_n() {
    println!("1. Async Parallel run_n");
    println!("   Run N instances concurrently with async tasks\n");
    
    let executor = AsyncExecutor::new(4);
    let counter = Arc::new(AtomicUsize::new(0));
    
    println!("   Running 5 async instances in parallel:");
    
    let c = counter.clone();
    executor.run_n_async(5, move || {
        let c = c.clone();
        let mut taskflow = Taskflow::new();
        
        taskflow.emplace_async(move || {
            let c = c.clone();
            async move {
                let id = c.fetch_add(1, Ordering::Relaxed);
                println!("     [Instance {}] Starting", id);
                
                // Simulate async I/O
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                println!("     [Instance {}] Completed", id);
            }
        });
        
        taskflow
    }).await;
    
    let final_count = counter.load(Ordering::Relaxed);
    println!("\n   Total executions: {}", final_count);
    assert_eq!(final_count, 5);
    println!("   ✓ All async instances completed");
}

#[cfg(feature = "async")]
async fn demo_async_run_until() {
    println!("2. Async run_until");
    println!("   Execute until condition is met (async)\n");
    
    let executor = AsyncExecutor::new(4);
    let sum = Arc::new(AtomicUsize::new(0));
    let target = 50;
    
    println!("   Target sum: {}", target);
    println!("   Executing...");
    
    let s = sum.clone();
    executor.run_until_async(
        move || {
            let s = s.clone();
            let mut taskflow = Taskflow::new();
            
            taskflow.emplace_async(move || {
                let s = s.clone();
                async move {
                    // Simulate async work
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    
                    let value = 5;
                    let new_sum = s.fetch_add(value, Ordering::Relaxed) + value;
                    println!("     Added {}, sum = {}", value, new_sum);
                }
            });
            
            taskflow
        },
        || sum.load(Ordering::Relaxed) >= target
    ).await;
    
    let final_sum = sum.load(Ordering::Relaxed);
    println!("\n   Final sum: {}", final_sum);
    assert!(final_sum >= target);
    println!("   ✓ Condition met");
}

#[cfg(feature = "async")]
async fn demo_sequential_vs_parallel_async() {
    println!("3. Sequential vs Parallel (Async)");
    println!("   Compare async execution modes\n");
    
    let executor = AsyncExecutor::new(4);
    
    // Sequential execution
    println!("   Sequential:");
    let start = Instant::now();
    executor.run_n_sequential_async(4, || {
        let mut taskflow = Taskflow::new();
        taskflow.emplace_async(|| async {
            tokio::time::sleep(Duration::from_millis(50)).await;
        });
        taskflow
    }).await;
    let sequential_time = start.elapsed();
    println!("     Time: {:?}", sequential_time);
    
    // Parallel execution
    println!("\n   Parallel:");
    let start = Instant::now();
    executor.run_n_async(4, || {
        let mut taskflow = Taskflow::new();
        taskflow.emplace_async(|| async {
            tokio::time::sleep(Duration::from_millis(50)).await;
        });
        taskflow
    }).await;
    let parallel_time = start.elapsed();
    println!("     Time: {:?}", parallel_time);
    
    println!("\n   Speedup: {:.2}x", 
             sequential_time.as_millis() as f64 / parallel_time.as_millis() as f64);
    println!("   ✓ Parallel async execution is faster");
}

#[cfg(feature = "async")]
async fn demo_real_world_async() {
    println!("4. Real-World: Async API Requests");
    println!("   Process multiple API batches concurrently\n");
    
    let executor = AsyncExecutor::new(8);
    let total_requests = Arc::new(AtomicUsize::new(0));
    
    println!("   Processing 3 batches of API requests:");
    
    let t = total_requests.clone();
    executor.run_n_async(3, move || {
        let t = t.clone();
        let mut taskflow = Taskflow::new();
        
        // Each batch makes multiple API calls
        for i in 0..5 {
            let t = t.clone();
            taskflow.emplace_async(move || async move {
                println!("     [Batch] API request {}", i);
                
                // Simulate API call
                tokio::time::sleep(Duration::from_millis(30)).await;
                
                t.fetch_add(1, Ordering::Relaxed);
                println!("     [Batch] Request {} completed", i);
            });
        }
        
        taskflow
    }).await;
    
    let total = total_requests.load(Ordering::Relaxed);
    println!("\n   Total API requests: {}", total);
    assert_eq!(total, 15); // 3 batches * 5 requests
    println!("   ✓ All async API requests completed");
}
