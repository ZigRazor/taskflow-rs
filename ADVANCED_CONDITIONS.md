# Advanced Conditional Features

This document describes the advanced conditional branching capabilities in TaskFlow-RS.

## Overview

TaskFlow-RS supports sophisticated control flow through:
1. **Multi-way branching**: Condition tasks can select from multiple execution paths
2. **Conditional handles**: Type-safe branch registration
3. **Dynamic workflow**: Runtime decision making in task graphs

## Basic Condition Tasks

Simple conditional tasks return a value that determines which successor runs:

```rust
let condition = taskflow.emplace_condition(|| {
    if check_something() { 0 } else { 1 }
});
```

## Multi-Way Branching

### Creating a Conditional Task

Use `emplace_conditional` to create a task that supports multiple branches:

```rust
let mut condition = taskflow.emplace_conditional(|| {
    // Return branch ID (0, 1, 2, ...)
    calculate_branch_id()
});
```

### Registering Branches

Associate tasks with specific branch IDs:

```rust
let task_a = taskflow.emplace(|| println!("Branch 0"));
let task_b = taskflow.emplace(|| println!("Branch 1"));
let task_c = taskflow.emplace(|| println!("Branch 2"));

condition.branch(0, &task_a);
condition.branch(1, &task_b);
condition.branch(2, &task_c);

// Must register branches before running
taskflow.register_branches(&condition);
```

### How It Works

1. The conditional task executes and returns a branch ID
2. The executor looks up which successors are registered for that branch
3. **Only tasks in the selected branch are activated**
4. Tasks in other branches are never executed

## Advanced Patterns

### Switch/Case Pattern

```rust
let mut mode_switch = taskflow.emplace_conditional(|| {
    match get_mode() {
        Mode::Debug => 0,
        Mode::Release => 1,
        Mode::Test => 2,
    }
});

// Each branch can have multiple tasks
let debug_setup = taskflow.emplace(|| /* ... */);
let debug_run = taskflow.emplace(|| /* ... */);
debug_setup.precede(&debug_run);

mode_switch.branch(0, &debug_setup);
// ... other branches

taskflow.register_branches(&mode_switch);
```

### Nested Conditionals

Conditions can be nested for complex decision trees:

```rust
// First level: Check user tier
let mut tier_check = taskflow.emplace_conditional(|| {
    if is_premium() { 0 } else { 1 }
});

// Second level: For premium users, check payment method
let mut payment_check = taskflow.emplace_conditional(|| {
    match get_payment_method() {
        PaymentMethod::Card => 0,
        PaymentMethod::PayPal => 1,
    }
});

tier_check.branch(0, payment_check.task());  // Premium → payment check
tier_check.branch(1, &upgrade_prompt);        // Free → upgrade

payment_check.branch(0, &process_card);
payment_check.branch(1, &process_paypal);

taskflow.register_branches(&tier_check);
taskflow.register_branches(&payment_check);
```

## Examples

See `examples/advanced_conditions.rs` for comprehensive demonstrations.

## Best Practices

1. **Always register branches** before running the taskflow
2. **Provide all expected branches** - missing branches will fall back to default behavior
3. **Keep branch logic simple** - complex conditions should be computed before the conditional task
4. **Document branch IDs** - use constants or enums for clarity

## Limitations

1. **Loop iterations**: No built-in iteration limit enforcement (use manual counters)

2. **Branch validation**: No compile-time validation of branch IDs

3. **Deadlock detection**: Cyclic dependencies may cause deadlocks if not carefully managed

## Advanced Features

### Multi-Task Branches

The executor automatically handles unreachable tasks in non-selected branches:

```rust
// ✅ Works correctly - executor skips unreachable tasks
let branch_a_1 = taskflow.emplace(|| println!("A1"));
let branch_a_2 = taskflow.emplace(|| println!("A2"));
branch_a_1.precede(&branch_a_2);

let branch_b_1 = taskflow.emplace(|| println!("B1"));
let branch_b_2 = taskflow.emplace(|| println!("B2"));
branch_b_1.precede(&branch_b_2);

condition.branch(0, &branch_a_1);  // Register first task of branch A
condition.branch(1, &branch_b_1);  // Register first task of branch B

// If branch A is selected, tasks branch_b_1 and branch_b_2 are
// automatically counted as unreachable and don't block completion
```

### Example of What NOT to Do

```rust
// ❌ WRONG - This will deadlock!
condition.branch(0, &task_a);
condition.branch(1, &task_b);

// final_task depends on BOTH task_a AND task_b
task_a.precede(&final_task);
task_b.precede(&final_task);

// Problem: Only ONE branch executes (either task_a OR task_b)
// but final_task waits for BOTH, causing a deadlock!
```

### Correct Patterns

**Option 1: Separate end tasks for each branch**
```rust
condition.branch(0, &task_a);
condition.branch(1, &task_b);

// Each branch has its own ending
task_a.precede(&end_a);
task_b.precede(&end_b);
```

**Option 2: Include the convergent task in each branch**
```rust
// Create a final task for each branch path
let final_a = taskflow.emplace(|| finish());
let final_b = taskflow.emplace(|| finish());

task_a.precede(&final_a);
task_b.precede(&final_b);

condition.branch(0, &task_a);
condition.branch(1, &task_b);
```
