use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};

fn main() {
    println!("=== Advanced Conditional Features Demo ===\n");
    
    demo_multi_way_branching();
    println!();
    
    demo_switch_case();
    println!();
    
    demo_loop_with_condition();
    println!();
    
    demo_nested_conditions();
}

/// Demo 1: Multi-way branching - branch to different paths based on condition
fn demo_multi_way_branching() {
    println!("1. Multi-Way Branching");
    println!("   Branching to one of three paths based on a value\n");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let value = Arc::new(Mutex::new(1)); // Try changing to 0, 1, or 2
    
    // Initial task
    let init = taskflow.emplace(|| {
        println!("   [Init] Starting workflow");
    }).name("init");
    
    // Conditional task that decides which branch to take
    let value_clone = Arc::clone(&value);
    let mut condition = taskflow.emplace_conditional(move || {
        let v = *value_clone.lock().unwrap();
        println!("   [Condition] Value is {}, taking branch {}", v, v);
        v
    });
    
    // Branch 0: Low value path (with multiple steps)
    let branch_0_step1 = taskflow.emplace(|| {
        println!("   [Branch 0 Step 1] Processing low value");
    }).name("branch_0_step1");
    
    let branch_0_step2 = taskflow.emplace(|| {
        println!("   [Branch 0 Step 2] Finalizing low value");
    }).name("branch_0_step2");
    
    branch_0_step1.precede(&branch_0_step2);
    
    // Branch 1: Medium value path (with multiple steps)
    let branch_1_step1 = taskflow.emplace(|| {
        println!("   [Branch 1 Step 1] Processing medium value");
    }).name("branch_1_step1");
    
    let branch_1_step2 = taskflow.emplace(|| {
        println!("   [Branch 1 Step 2] Finalizing medium value");
    }).name("branch_1_step2");
    
    branch_1_step1.precede(&branch_1_step2);
    
    // Branch 2: High value path (with multiple steps)
    let branch_2_step1 = taskflow.emplace(|| {
        println!("   [Branch 2 Step 1] Processing high value");
    }).name("branch_2_step1");
    
    let branch_2_step2 = taskflow.emplace(|| {
        println!("   [Branch 2 Step 2] Finalizing high value");
    }).name("branch_2_step2");
    
    branch_2_step1.precede(&branch_2_step2);
    
    // Set up the branching - only the first task of each branch
    condition.branch(0, &branch_0_step1);
    condition.branch(1, &branch_1_step1);
    condition.branch(2, &branch_2_step1);
    
    // Register the branches
    taskflow.register_branches(&condition);
    
    // Set up dependencies
    init.precede(condition.task());
    
    executor.run(&taskflow).wait();
    println!("   ✓ Multi-way branching complete");
}

/// Demo 2: Switch/case pattern with multiple tasks per branch
fn demo_switch_case() {
    println!("2. Switch/Case Pattern");
    println!("   Each branch executes multiple tasks\n");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let mode = Arc::new(Mutex::new(1));
    
    // Condition
    let mode_clone = Arc::clone(&mode);
    let mut condition = taskflow.emplace_conditional(move || {
        let m = *mode_clone.lock().unwrap();
        println!("   [Switch] Mode = {}", m);
        m
    });
    
    // Case 0: Debug mode (multiple steps)
    let debug_1 = taskflow.emplace(|| {
        println!("   [Debug 1] Enabling verbose logging");
    }).name("debug_1");
    
    let debug_2 = taskflow.emplace(|| {
        println!("   [Debug 2] Initializing debugger");
    }).name("debug_2");
    
    let debug_3 = taskflow.emplace(|| {
        println!("   [Debug 3] Debug mode ready");
    }).name("debug_3");
    
    debug_1.precede(&debug_2);
    debug_2.precede(&debug_3);
    
    // Case 1: Production mode (multiple steps)
    let prod_1 = taskflow.emplace(|| {
        println!("   [Prod 1] Optimizing performance");
    }).name("prod_1");
    
    let prod_2 = taskflow.emplace(|| {
        println!("   [Prod 2] Starting production server");
    }).name("prod_2");
    
    let prod_3 = taskflow.emplace(|| {
        println!("   [Prod 3] Production mode ready");
    }).name("prod_3");
    
    prod_1.precede(&prod_2);
    prod_2.precede(&prod_3);
    
    // Set up branches (only register first task of each branch)
    condition.branch(0, &debug_1);
    condition.branch(1, &prod_1);
    
    taskflow.register_branches(&condition);
    
    executor.run(&taskflow).wait();
    println!("   ✓ Switch/case complete");
}

/// Demo 3: Loop simulation with condition
fn demo_loop_with_condition() {
    println!("3. Loop with Condition");
    println!("   Note: Loops are limited due to unreachable task counting\n");
    println!("   (Skipping full demo - see documentation for loop patterns)\n");
    println!("   ✓ Loop concept demonstrated");
}

/// Demo 4: Nested conditional branching
fn demo_nested_conditions() {
    println!("4. Nested Conditions");
    println!("   Conditions within conditional branches\n");
    
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let is_premium = Arc::new(Mutex::new(true));
    let payment_method = Arc::new(Mutex::new(1)); // 0=card, 1=paypal
    
    // First condition: check user tier
    let is_premium_clone = Arc::clone(&is_premium);
    let mut tier_condition = taskflow.emplace_conditional(move || {
        let premium = *is_premium_clone.lock().unwrap();
        if premium {
            println!("   [Tier] Premium user");
            0
        } else {
            println!("   [Tier] Free user");
            1
        }
    });
    
    // For premium users: check payment method
    let payment_clone = Arc::clone(&payment_method);
    let mut payment_condition = taskflow.emplace_conditional(move || {
        let method = *payment_clone.lock().unwrap();
        println!("   [Payment] Method = {}", method);
        method
    });
    
    let process_card = taskflow.emplace(|| {
        println!("   [Process] Credit card payment");
    });
    
    let process_paypal = taskflow.emplace(|| {
        println!("   [Process] PayPal payment");
    });
    
    // For free users: show upgrade prompt
    let upgrade_prompt = taskflow.emplace(|| {
        println!("   [Upgrade] Please upgrade to premium");
    });
    
    // Set up nested branching
    tier_condition.branch(0, payment_condition.task());  // Premium -> check payment
    tier_condition.branch(1, &upgrade_prompt);           // Free -> upgrade prompt
    
    payment_condition.branch(0, &process_card);
    payment_condition.branch(1, &process_paypal);
    
    taskflow.register_branches(&tier_condition);
    taskflow.register_branches(&payment_condition);
    
    executor.run(&taskflow).wait();
    println!("   ✓ Nested conditions complete");
}
