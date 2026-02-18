use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};

fn main() {
    println!("=== Parallel Pattern Examples ===\n");
    
    parallel_for_pattern();
    println!();
    
    map_reduce_pattern();
    println!();
    
    pipeline_pattern();
}

/// Example: Parallel for pattern using task graph
fn parallel_for_pattern() {
    println!("1. Parallel For Pattern");
    println!("Processing array elements in parallel...");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let data = Arc::new(Mutex::new(vec![0i32; 100]));
    let chunk_size = 25;
    
    let mut tasks = Vec::new();
    
    // Create tasks for each chunk
    for i in 0..4 {
        let data = Arc::clone(&data);
        let task = taskflow.emplace(move || {
            let start = i * chunk_size;
            let end = start + chunk_size;
            let mut data = data.lock().unwrap();
            
            for j in start..end {
                data[j] = (j * 2) as i32; // Example computation
            }
            
            println!("  Chunk {} processed (elements {}-{})", i, start, end - 1);
        }).name(&format!("chunk_{}", i));
        
        tasks.push(task);
    }
    
    // Create a final task that depends on all chunks
    let data_clone = Arc::clone(&data);
    let finalize = taskflow.emplace(move || {
        let data = data_clone.lock().unwrap();
        println!("  Finalize: Sum of first 10 elements = {}", 
                 data[0..10].iter().sum::<i32>());
    }).name("finalize");
    
    for task in &tasks {
        task.precede(&finalize);
    }
    
    executor.run(&taskflow).wait();
}

/// Example: Map-reduce pattern
fn map_reduce_pattern() {
    println!("2. Map-Reduce Pattern");
    println!("Computing sum in parallel...");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let input = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let num_chunks = 4;
    let chunk_size = input.len() / num_chunks;
    
    let partial_sums = Arc::new(Mutex::new(vec![0i32; num_chunks]));
    let mut map_tasks = Vec::new();
    
    // Map phase: compute partial sums
    for i in 0..num_chunks {
        let input = input.clone();
        let partial_sums = Arc::clone(&partial_sums);
        
        let task = taskflow.emplace(move || {
            let start = i * chunk_size;
            let end = if i == num_chunks - 1 { 
                input.len() 
            } else { 
                start + chunk_size 
            };
            
            let sum: i32 = input[start..end].iter().sum();
            partial_sums.lock().unwrap()[i] = sum;
            
            println!("  Map {}: Sum of elements {}-{} = {}", i, start, end - 1, sum);
        }).name(&format!("map_{}", i));
        
        map_tasks.push(task);
    }
    
    // Reduce phase: combine partial sums
    let partial_sums_clone = Arc::clone(&partial_sums);
    let reduce = taskflow.emplace(move || {
        let sums = partial_sums_clone.lock().unwrap();
        let total: i32 = sums.iter().sum();
        println!("  Reduce: Total sum = {}", total);
    }).name("reduce");
    
    for task in &map_tasks {
        task.precede(&reduce);
    }
    
    executor.run(&taskflow).wait();
}

/// Example: Pipeline pattern (simulated)
fn pipeline_pattern() {
    println!("3. Pipeline Pattern (3 stages)");
    println!("Processing tokens through pipeline...");
    
    let mut executor = Executor::new(4);
    let num_tokens = 5;
    
    for token in 0..num_tokens {
        let mut taskflow = Taskflow::new();
        
        let token_data = Arc::new(Mutex::new(token));
        
        // Stage 1: Generate
        let data1 = Arc::clone(&token_data);
        let stage1 = taskflow.emplace(move || {
            let mut data = data1.lock().unwrap();
            *data = *data * 2;
            println!("  Token {}: Stage 1 (Generate) -> {}", token, *data);
        }).name(&format!("token_{}_stage_1", token));
        
        // Stage 2: Transform
        let data2 = Arc::clone(&token_data);
        let stage2 = taskflow.emplace(move || {
            let mut data = data2.lock().unwrap();
            *data = *data + 10;
            println!("  Token {}: Stage 2 (Transform) -> {}", token, *data);
        }).name(&format!("token_{}_stage_2", token));
        
        // Stage 3: Consume
        let data3 = Arc::clone(&token_data);
        let stage3 = taskflow.emplace(move || {
            let data = data3.lock().unwrap();
            println!("  Token {}: Stage 3 (Consume) -> Final value = {}", token, *data);
        }).name(&format!("token_{}_stage_3", token));
        
        stage1.precede(&stage2);
        stage2.precede(&stage3);
        
        executor.run(&taskflow).wait();
    }
}
