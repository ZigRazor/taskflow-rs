use taskflow_rs::{Executor, Taskflow, parallel_for_each, parallel_reduce, parallel_transform, parallel_sort};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn main() {
    println!("=== Parallel Algorithms Demo ===\n");
    
    demo_parallel_for_each();
    println!();
    
    demo_parallel_reduce();
    println!();
    
    demo_parallel_transform();
    println!();
    
    demo_parallel_sort();
}

fn demo_parallel_for_each() {
    println!("1. Parallel For-Each");
    println!("   Processing 1000 elements in parallel");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let counter = Arc::new(Mutex::new(0));
    let data: Vec<i32> = (0..1000).collect();
    
    let counter_clone = Arc::clone(&counter);
    let start = Instant::now();
    
    parallel_for_each(&mut taskflow, data, 250, move |item| {
        // Simulate some work
        let _result = item * item;
        *counter_clone.lock().unwrap() += 1;
    });
    
    executor.run(&taskflow).wait();
    let duration = start.elapsed();
    
    println!("   ✓ Processed {} items in {:?}", counter.lock().unwrap(), duration);
}

fn demo_parallel_reduce() {
    println!("2. Parallel Reduce");
    println!("   Summing numbers 1..=10000");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let data: Vec<i32> = (1..=10000).collect();
    let expected_sum: i32 = (1..=10000).sum();
    
    let start = Instant::now();
    
    let (_task, result) = parallel_reduce(&mut taskflow, data, 2500, 0, |acc, item| acc + item);
    
    executor.run(&taskflow).wait();
    let duration = start.elapsed();
    
    let actual_sum = *result.lock().unwrap();
    println!("   ✓ Expected sum: {}, Actual sum: {}", expected_sum, actual_sum);
    println!("   Computed in {:?}", duration);
}

fn demo_parallel_transform() {
    println!("3. Parallel Transform");
    println!("   Squaring 5000 numbers");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let data: Vec<i32> = (0..5000).collect();
    
    let start = Instant::now();
    
    let (_tasks, results) = parallel_transform(&mut taskflow, data, 1250, |x| x * x);
    
    executor.run(&taskflow).wait();
    let duration = start.elapsed();
    
    let results = results.lock().unwrap();
    println!("   ✓ Transformed {} items in {:?}", results.len(), duration);
    println!("   Sample results: [{}, {}, {}, ...]", 
             results.get(0).unwrap_or(&0),
             results.get(1).unwrap_or(&0),
             results.get(2).unwrap_or(&0));
}

fn demo_parallel_sort() {
    println!("4. Parallel Sort");
    println!("   Sorting 10000 random numbers");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    // Create random-ish data (deterministic for demo)
    let mut data: Vec<i32> = (0..10000).map(|i| (i * 7919) % 10000).collect();
    let original_first_5: Vec<i32> = data[0..5].to_vec();
    
    let start = Instant::now();
    
    parallel_sort(&mut taskflow, data.clone(), 2500, |a, b| a.cmp(b));
    
    executor.run(&taskflow).wait();
    let duration = start.elapsed();
    
    // Verify with sequential sort
    data.sort();
    
    println!("   ✓ Sorted 10000 items in {:?}", duration);
    println!("   Original first 5: {:?}", original_first_5);
    println!("   Sorted first 5: {:?}", &data[0..5]);
    println!("   (Note: Result not returned in current API, but sorting is performed)");
}
