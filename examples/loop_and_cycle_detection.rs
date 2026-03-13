use std::sync::{Arc, Mutex};
use taskflow_rs::{CycleDetector, Executor, Loop, Taskflow};

fn main() {
    println!("=== Cycle Detection and Loop Support Demo ===\n");

    demo_cycle_detection();
    println!();

    demo_loop_construct();
    println!();

    demo_controlled_iteration();
    println!();

    demo_complex_graph();
}

/// Demo 1: Cycle Detection
fn demo_cycle_detection() {
    println!("1. Cycle Detection");
    println!("   Detecting cycles in task graphs\n");

    // Test 1: No cycle (DAG)
    {
        println!("   Test 1: Linear chain (no cycle)");
        let mut detector = CycleDetector::new();

        // 1 -> 2 -> 3 -> 4
        detector.add_dependency(1, 2);
        detector.add_dependency(2, 3);
        detector.add_dependency(3, 4);

        let result = detector.detect_cycle();
        println!("     Graph: 1 -> 2 -> 3 -> 4");
        println!("     Has cycle: {}", result.has_cycle());
        assert!(!result.has_cycle());

        // Get topological sort
        if let Some(sorted) = detector.topological_sort() {
            println!("     Topological order: {:?}", sorted);
        }
    }

    // Test 2: Simple cycle
    {
        println!("\n   Test 2: Simple cycle");
        let mut detector = CycleDetector::new();

        // 1 -> 2 -> 3 -> 1 (cycle!)
        detector.add_dependency(1, 2);
        detector.add_dependency(2, 3);
        detector.add_dependency(3, 1);

        let result = detector.detect_cycle();
        println!("     Graph: 1 -> 2 -> 3 -> 1");
        println!("     Has cycle: {}", result.has_cycle());

        if let Some(cycle) = result.cycle_path() {
            println!("     Cycle path: {:?}", cycle);
        }

        assert!(result.has_cycle());

        // Topological sort should fail
        assert!(detector.topological_sort().is_none());
        println!("     Topological sort: Failed (cycle detected)");
    }

    // Test 3: Self-loop
    {
        println!("\n   Test 3: Self-loop");
        let mut detector = CycleDetector::new();

        // 1 -> 1 (self loop!)
        detector.add_dependency(1, 1);

        let result = detector.detect_cycle();
        println!("     Graph: 1 -> 1");
        println!("     Has cycle: {}", result.has_cycle());
        assert!(result.has_cycle());
    }

    // Test 4: Diamond (no cycle)
    {
        println!("\n   Test 4: Diamond pattern (no cycle)");
        let mut detector = CycleDetector::new();

        // 1 -> 2 -> 4
        // 1 -> 3 -> 4
        detector.add_dependency(1, 2);
        detector.add_dependency(1, 3);
        detector.add_dependency(2, 4);
        detector.add_dependency(3, 4);

        let result = detector.detect_cycle();
        println!("     Graph: Diamond (1 branches to 2,3 which merge at 4)");
        println!("     Has cycle: {}", result.has_cycle());
        assert!(!result.has_cycle());
    }

    println!("\n   ✓ Cycle detection working correctly");
}

/// Demo 2: Loop Construct
fn demo_loop_construct() {
    println!("2. Loop Construct");
    println!("   Using explicit loop constructs\n");

    let counter = Arc::new(Mutex::new(0));
    let max_iterations = 5;

    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    // Create loop condition
    let c = counter.clone();
    let condition = taskflow
        .emplace(move || {
            let count = *c.lock().unwrap();
            println!("     [Condition] Iteration {}", count);
            // Note: In full implementation, this would evaluate condition
            // For now, we just log the check
        })
        .name("loop_condition");

    // Create loop body
    let c = counter.clone();
    let body_task = taskflow
        .emplace(move || {
            let mut count = c.lock().unwrap();
            *count += 1;
            println!("     [Body] Processing iteration {}", *count);
            std::thread::sleep(std::time::Duration::from_millis(50));
        })
        .name("loop_body");

    // Create loop structure
    let mut loop_construct = Loop::new(condition.clone());
    loop_construct.add_body_task(body_task.clone());
    loop_construct.max_iterations(10); // Safety limit

    println!("   Loop structure created:");
    println!("     Condition: Check counter < {}", max_iterations);
    println!("     Body: Increment counter and process");
    println!("     Max iterations: 10 (safety limit)");
    println!("     Note: Full runtime loop execution requires executor integration");
    println!("\n   Simulating loop with controlled execution...\n");

    // Note: Full loop execution would require runtime support
    // For now, demonstrate controlled iteration
    for i in 0..max_iterations {
        let mut iter_taskflow = Taskflow::new();

        let c = counter.clone();
        iter_taskflow.emplace(move || {
            let mut count = c.lock().unwrap();
            *count = i + 1;
            println!("     [Iteration {}] Counter = {}", i + 1, *count);
        });

        executor.run(&iter_taskflow).wait();
    }

    let final_count = *counter.lock().unwrap();
    println!("\n   Final counter value: {}", final_count);
    println!("   ✓ Loop completed successfully");
}

/// Demo 3: Controlled Iteration
fn demo_controlled_iteration() {
    println!("3. Controlled Iteration");
    println!("   Iterative processing with controlled loops\n");

    let data = Arc::new(Mutex::new(vec![10, 20, 30, 40, 50]));
    let processed = Arc::new(Mutex::new(Vec::new()));

    let mut executor = Executor::new(4);

    println!("   Input data: {:?}", *data.lock().unwrap());
    println!("   Processing each element...\n");

    // Process each element in sequence
    for i in 0..5 {
        let mut taskflow = Taskflow::new();
        let d = data.clone();
        let p = processed.clone();

        taskflow.emplace(move || {
            let data = d.lock().unwrap();
            let value = data[i];
            let result = value * 2;

            println!("     [Iteration {}] {} * 2 = {}", i + 1, value, result);

            p.lock().unwrap().push(result);
        });

        executor.run(&taskflow).wait();
    }

    println!("\n   Processed results: {:?}", *processed.lock().unwrap());
    println!("   ✓ Controlled iteration complete");
}

/// Demo 4: Complex Graph Analysis
fn demo_complex_graph() {
    println!("4. Complex Graph Analysis");
    println!("   Analyzing complex task dependencies\n");

    let mut detector = CycleDetector::new();

    // Build a complex graph
    // Layer 1: Tasks 1, 2
    // Layer 2: Tasks 3, 4, 5
    // Layer 3: Task 6
    detector.add_dependency(1, 3);
    detector.add_dependency(1, 4);
    detector.add_dependency(2, 4);
    detector.add_dependency(2, 5);
    detector.add_dependency(3, 6);
    detector.add_dependency(4, 6);
    detector.add_dependency(5, 6);

    println!("   Graph structure:");
    println!("     Layer 1: [1] -> [3, 4]");
    println!("              [2] -> [4, 5]");
    println!("     Layer 2: [3, 4, 5] -> [6]");

    let result = detector.detect_cycle();
    println!(
        "\n   Cycle detection: {}",
        if result.has_cycle() {
            "CYCLE FOUND"
        } else {
            "NO CYCLE"
        }
    );

    if let Some(sorted) = detector.topological_sort() {
        println!("   Topological order: {:?}", sorted);
        println!("   ✓ Valid execution order found");
    } else {
        println!("   ✗ Cannot create execution order (cycle detected)");
    }

    // Test with a cycle added
    println!("\n   Adding cycle: 6 -> 1");
    detector.add_dependency(6, 1);

    let result = detector.detect_cycle();
    println!(
        "   Cycle detection: {}",
        if result.has_cycle() {
            "CYCLE FOUND"
        } else {
            "NO CYCLE"
        }
    );

    if let Some(cycle) = result.cycle_path() {
        println!("   Cycle path: {:?}", cycle);
        println!("   ✓ Cycle correctly detected");
    }
}
