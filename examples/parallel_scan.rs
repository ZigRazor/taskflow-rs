use taskflow_rs::{Executor, Taskflow, parallel_inclusive_scan, parallel_exclusive_scan};

fn main() {
    println!("=== Parallel Scan Demo ===\n");
    
    demo_inclusive_scan();
    println!();
    
    demo_exclusive_scan();
    println!();
    
    demo_scan_operations();
    println!();
    
    demo_performance();
}

/// Demo 1: Inclusive Scan (Cumulative Sum)
fn demo_inclusive_scan() {
    println!("1. Inclusive Scan (Prefix Sum)");
    println!("   Each output[i] = sum of input[0..=i]\n");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
    println!("   Input:  {:?}", data);
    
    let (_tasks, result) = parallel_inclusive_scan(
        &mut taskflow,
        data,
        2,  // chunk size
        |a, b| a + b,  // addition
        0   // identity
    );
    
    executor.run(&taskflow).wait();
    
    let output = result.lock().unwrap();
    println!("   Output: {:?}", *output);
    println!("   Expected: [1, 3, 6, 10, 15, 21, 28, 36]");
    
    assert_eq!(*output, vec![1, 3, 6, 10, 15, 21, 28, 36]);
    println!("   ✓ Inclusive scan correct");
}

/// Demo 2: Exclusive Scan (Shifted Cumulative Sum)
fn demo_exclusive_scan() {
    println!("2. Exclusive Scan");
    println!("   Each output[i] = sum of input[0..i] (excluding i)\n");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
    println!("   Input:  {:?}", data);
    
    let (_tasks, result) = parallel_exclusive_scan(
        &mut taskflow,
        data,
        2,
        |a, b| a + b,
        0
    );
    
    executor.run(&taskflow).wait();
    
    let output = result.lock().unwrap();
    println!("   Output: {:?}", *output);
    println!("   Expected: [0, 1, 3, 6, 10, 15, 21, 28]");
    
    assert_eq!(*output, vec![0, 1, 3, 6, 10, 15, 21, 28]);
    println!("   ✓ Exclusive scan correct");
}

/// Demo 3: Different Operations
fn demo_scan_operations() {
    println!("3. Scan with Different Operations");
    
    let mut executor = Executor::new(4);
    
    // Multiplication scan
    {
        println!("\n   Multiplication Scan:");
        let mut taskflow = Taskflow::new();
        let data = vec![2, 3, 4, 5];
        println!("   Input:  {:?}", data);
        
        let (_tasks, result) = parallel_inclusive_scan(
            &mut taskflow,
            data,
            2,
            |a, b| a * b,
            1  // identity for multiplication
        );
        
        executor.run(&taskflow).wait();
        
        let output = result.lock().unwrap();
        println!("   Output: {:?}", *output);
        println!("   Expected: [2, 6, 24, 120]");
        
        assert_eq!(*output, vec![2, 6, 24, 120]);
    }
    
    // Maximum scan
    {
        println!("\n   Maximum Scan:");
        let mut taskflow = Taskflow::new();
        let data = vec![3, 1, 4, 1, 5, 9, 2, 6];
        println!("   Input:  {:?}", data);
        
        let (_tasks, result) = parallel_inclusive_scan(
            &mut taskflow,
            data,
            2,
            |a, b| a.max(b),
            i32::MIN
        );
        
        executor.run(&taskflow).wait();
        
        let output = result.lock().unwrap();
        println!("   Output: {:?}", *output);
        println!("   Expected: [3, 3, 4, 4, 5, 9, 9, 9]");
        
        assert_eq!(*output, vec![3, 3, 4, 4, 5, 9, 9, 9]);
    }
    
    println!("\n   ✓ Different operations work correctly");
}

/// Demo 4: Performance with Large Dataset
fn demo_performance() {
    println!("4. Performance Test");
    
    // Test with i32 for smaller sizes
    println!("\n   i32 tests:");
    let sizes = vec![100, 1_000, 10_000];
    
    for size in sizes {
        let mut executor = Executor::new(8);
        let mut taskflow = Taskflow::new();
        
        let data: Vec<i32> = (1..=size).collect();
        
        let start = std::time::Instant::now();
        
        let (_tasks, result) = parallel_inclusive_scan(
            &mut taskflow,
            data,
            1000,  // chunk size
            |a, b| a + b,
            0
        );
        
        executor.run(&taskflow).wait();
        
        let elapsed = start.elapsed();
        
        // Verify correctness (sum of 1..n = n*(n+1)/2)
        let expected_sum = ((size as i64) * (size as i64 + 1)) / 2;
        let actual_sum = result.lock().unwrap()[size as usize - 1] as i64;
        
        println!("   Size: {:>7} | Time: {:>8.2?} | Sum: {} | Correct: {}", 
                 size, elapsed, actual_sum, actual_sum == expected_sum);
    }
    
    // Test with i64 for larger size to avoid overflow
    println!("\n   i64 tests:");
    let sizes = vec![100_000, 1_000_000];
    
    for size in sizes {
        let mut executor = Executor::new(8);
        let mut taskflow = Taskflow::new();
        
        let data: Vec<i64> = (1..=size).collect();
        
        let start = std::time::Instant::now();
        
        let (_tasks, result) = parallel_inclusive_scan(
            &mut taskflow,
            data,
            10000,  // larger chunk size for bigger data
            |a, b| a + b,
            0i64
        );
        
        executor.run(&taskflow).wait();
        
        let elapsed = start.elapsed();
        
        // Verify correctness (sum of 1..n = n*(n+1)/2)
        let expected_sum = (size * (size + 1)) / 2;
        let actual_sum = result.lock().unwrap()[size as usize - 1];
        
        println!("   Size: {:>7} | Time: {:>8.2?} | Sum: {} | Correct: {}", 
                 size, elapsed, actual_sum, actual_sum == expected_sum);
    }
    
    println!("\n   ✓ Performance test complete");
}
