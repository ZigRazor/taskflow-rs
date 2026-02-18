use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};

fn main() {
    println!("Starting minimal test...");
    
    // Test 1: Single task
    println!("\n1. Testing single task:");
    {
        let mut executor = Executor::new(1);
        let mut taskflow = Taskflow::new();
        
        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);
        
        taskflow.emplace(move || {
            println!("  Task executed!");
            *executed_clone.lock().unwrap() = true;
        });
        
        println!("  Running taskflow...");
        executor.run(&taskflow).wait();
        println!("  Done!");
        
        assert!(*executed.lock().unwrap(), "Task was not executed!");
        println!("  ✓ Single task works");
    }
    
    // Test 2: Two independent tasks
    println!("\n2. Testing two independent tasks:");
    {
        let mut executor = Executor::new(2);
        let mut taskflow = Taskflow::new();
        
        let counter = Arc::new(Mutex::new(0));
        
        let c1 = Arc::clone(&counter);
        taskflow.emplace(move || {
            println!("  Task 1 executed!");
            *c1.lock().unwrap() += 1;
        });
        
        let c2 = Arc::clone(&counter);
        taskflow.emplace(move || {
            println!("  Task 2 executed!");
            *c2.lock().unwrap() += 1;
        });
        
        println!("  Running taskflow...");
        executor.run(&taskflow).wait();
        println!("  Done!");
        
        assert_eq!(*counter.lock().unwrap(), 2, "Not all tasks executed!");
        println!("  ✓ Two independent tasks work");
    }
    
    // Test 3: Two tasks with dependency
    println!("\n3. Testing two tasks with dependency:");
    {
        let mut executor = Executor::new(2);
        let mut taskflow = Taskflow::new();
        
        let order = Arc::new(Mutex::new(Vec::new()));
        
        let o1 = Arc::clone(&order);
        let a = taskflow.emplace(move || {
            println!("  Task A executed!");
            o1.lock().unwrap().push(1);
        }).name("A");
        
        let o2 = Arc::clone(&order);
        let b = taskflow.emplace(move || {
            println!("  Task B executed!");
            o2.lock().unwrap().push(2);
        }).name("B");
        
        a.precede(&b);
        
        println!("  Running taskflow...");
        executor.run(&taskflow).wait();
        println!("  Done!");
        
        let result = order.lock().unwrap();
        assert_eq!(*result, vec![1, 2], "Tasks executed in wrong order!");
        println!("  ✓ Dependency works");
    }
    
    println!("\n✅ All basic tests passed!");
}
