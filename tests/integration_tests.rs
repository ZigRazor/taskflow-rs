use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

#[test]
fn test_simple_task() {
    let mut executor = Executor::new(1);
    let mut taskflow = Taskflow::new();
    
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = Arc::clone(&executed);
    
    taskflow.emplace(move || {
        *executed_clone.lock().unwrap() = true;
    });
    
    executor.run(&taskflow).wait();
    
    assert!(*executed.lock().unwrap());
}

#[test]
fn test_sequential_tasks() {
    let mut executor = Executor::new(2);
    let mut taskflow = Taskflow::new();
    
    let order = Arc::new(Mutex::new(Vec::new()));
    
    let order1 = Arc::clone(&order);
    let a = taskflow.emplace(move || {
        order1.lock().unwrap().push(1);
    }).name("A");
    
    let order2 = Arc::clone(&order);
    let b = taskflow.emplace(move || {
        order2.lock().unwrap().push(2);
    }).name("B");
    
    a.precede(&b);
    
    executor.run(&taskflow).wait();
    
    let final_order = order.lock().unwrap();
    assert_eq!(*final_order, vec![1, 2]);
}

#[test]
fn test_diamond_pattern() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let order = Arc::new(Mutex::new(Vec::new()));
    
    // Create start task
    let order_clone = Arc::clone(&order);
    let start = taskflow.emplace(move || {
        order_clone.lock().unwrap().push(0);
        thread::sleep(Duration::from_millis(10));
    }).name("start");
    
    // Create parallel tasks
    let order_clone = Arc::clone(&order);
    let left = taskflow.emplace(move || {
        thread::sleep(Duration::from_millis(50));
        order_clone.lock().unwrap().push(1);
    }).name("left");
    
    let order_clone = Arc::clone(&order);
    let right = taskflow.emplace(move || {
        thread::sleep(Duration::from_millis(50));
        order_clone.lock().unwrap().push(2);
    }).name("right");
    
    // Create end task
    let order_clone = Arc::clone(&order);
    let end = taskflow.emplace(move || {
        order_clone.lock().unwrap().push(3);
    }).name("end");
    
    // Build diamond
    start.precede(&left);
    start.precede(&right);
    end.succeed(&left);  // end runs after left (left -> end)
    end.succeed(&right); // end runs after right (right -> end)
    
    executor.run(&taskflow).wait();
    
    let final_order = order.lock().unwrap();
    
    // Start should be first, end should be last
    assert_eq!(final_order[0], 0);
    assert_eq!(final_order[3], 3);
    
    // Left and right can be in any order
    assert!(final_order[1..3].contains(&1));
    assert!(final_order[1..3].contains(&2));
}

#[test]
fn test_multiple_successors() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let counter = Arc::new(Mutex::new(0));
    
    let counter_clone = Arc::clone(&counter);
    let a = taskflow.emplace(move || {
        *counter_clone.lock().unwrap() += 1;
    }).name("A");
    
    let counter_clone = Arc::clone(&counter);
    let b = taskflow.emplace(move || {
        *counter_clone.lock().unwrap() += 1;
    }).name("B");
    
    let counter_clone = Arc::clone(&counter);
    let c = taskflow.emplace(move || {
        *counter_clone.lock().unwrap() += 1;
    }).name("C");
    
    let counter_clone = Arc::clone(&counter);
    let d = taskflow.emplace(move || {
        *counter_clone.lock().unwrap() += 1;
    }).name("D");
    
    a.precede(&b);
    a.precede(&c);
    a.precede(&d);
    
    executor.run(&taskflow).wait();
    
    assert_eq!(*counter.lock().unwrap(), 4);
}

#[test]
fn test_taskflow_size() {
    let mut taskflow = Taskflow::new();
    
    assert_eq!(taskflow.size(), 0);
    assert!(taskflow.is_empty());
    
    taskflow.emplace(|| {});
    assert_eq!(taskflow.size(), 1);
    assert!(!taskflow.is_empty());
    
    taskflow.emplace(|| {});
    taskflow.emplace(|| {});
    assert_eq!(taskflow.size(), 3);
}

#[test]
fn test_dump_graph() {
    let mut taskflow = Taskflow::new();
    
    let a = taskflow.emplace(|| {}).name("A");
    let b = taskflow.emplace(|| {}).name("B");
    
    a.precede(&b);
    
    let dot = taskflow.dump();
    
    assert!(dot.contains("digraph Taskflow"));
    assert!(dot.contains("A"));
    assert!(dot.contains("B"));
    assert!(dot.contains("->"));
}

#[test]
fn test_subflow() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let parent_executed = Arc::new(Mutex::new(false));
    let child1_executed = Arc::new(Mutex::new(false));
    let child2_executed = Arc::new(Mutex::new(false));
    
    let parent_clone = Arc::clone(&parent_executed);
    let child1_clone = Arc::clone(&child1_executed);
    let child2_clone = Arc::clone(&child2_executed);
    
    taskflow.emplace_subflow(move |subflow| {
        *parent_clone.lock().unwrap() = true;
        
        let c1_clone = Arc::clone(&child1_clone);
        subflow.emplace(move || {
            *c1_clone.lock().unwrap() = true;
        });
        
        let c2_clone = Arc::clone(&child2_clone);
        subflow.emplace(move || {
            *c2_clone.lock().unwrap() = true;
        });
    });
    
    executor.run(&taskflow).wait();
    
    assert!(*parent_executed.lock().unwrap());
    assert!(*child1_executed.lock().unwrap());
    assert!(*child2_executed.lock().unwrap());
}

#[test]
fn test_complex_graph() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let executed = Arc::new(Mutex::new(Vec::new()));
    
    let mut tasks = Vec::new();
    for i in 0..10 {
        let exec_clone = Arc::clone(&executed);
        let task = taskflow.emplace(move || {
            exec_clone.lock().unwrap().push(i);
        }).name(&format!("task_{}", i));
        tasks.push(task);
    }
    
    // Create a more complex dependency graph
    tasks[0].precede(&tasks[1]);
    tasks[0].precede(&tasks[2]);
    tasks[1].precede(&tasks[3]);
    tasks[2].precede(&tasks[3]);
    tasks[3].precede(&tasks[4]);
    tasks[3].precede(&tasks[5]);
    tasks[4].precede(&tasks[6]);
    tasks[5].precede(&tasks[6]);
    tasks[6].precede(&tasks[7]);
    tasks[7].precede(&tasks[8]);
    tasks[8].precede(&tasks[9]);
    
    executor.run(&taskflow).wait();
    
    let exec = executed.lock().unwrap();
    assert_eq!(exec.len(), 10);
    
    // Verify task 0 executed before 1 and 2
    assert!(exec.iter().position(|&x| x == 0) < exec.iter().position(|&x| x == 1));
    assert!(exec.iter().position(|&x| x == 0) < exec.iter().position(|&x| x == 2));
    
    // Verify task 9 executed last
    assert_eq!(exec[9], 9);
}
