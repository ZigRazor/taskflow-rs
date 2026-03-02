# Loop Support and Cycle Detection

TaskFlow-RS provides comprehensive cycle detection and loop constructs for building complex iterative workflows.

## Features

- **Cycle Detection** - DFS-based algorithm to detect cycles in task graphs
- **Topological Sort** - Order tasks for execution (fails if cycles exist)
- **Loop Constructs** - Explicit loop support with iteration control
- **Strongly Connected Components** - Advanced cycle analysis
- **Safety Limits** - Maximum iteration bounds to prevent infinite loops

## Cycle Detection

### Why Cycle Detection Matters

Task graphs must be **Directed Acyclic Graphs (DAGs)** to execute correctly:
- ✅ **DAG**: Tasks execute in topological order
- ❌ **Cycle**: Deadlock - tasks wait for each other indefinitely

### Basic Usage

```rust
use taskflow_rs::CycleDetector;

let mut detector = CycleDetector::new();

// Add dependencies
detector.add_dependency(1, 2);  // 1 -> 2
detector.add_dependency(2, 3);  // 2 -> 3
detector.add_dependency(3, 1);  // 3 -> 1 (creates cycle!)

// Detect cycles
let result = detector.detect_cycle();

if result.has_cycle() {
    println!("Cycle detected!");
    if let Some(path) = result.cycle_path() {
        println!("Cycle path: {:?}", path);
    }
}
```

### Cycle Detection API

```rust
pub enum CycleDetectionResult {
    NoCycle,
    CycleDetected(Vec<TaskId>),
}

impl CycleDetectionResult {
    pub fn has_cycle(&self) -> bool;
    pub fn cycle_path(&self) -> Option<&[TaskId]>;
}
```

## Examples

### Example 1: Linear Chain (No Cycle)

```rust
let mut detector = CycleDetector::new();

// Linear: 1 -> 2 -> 3 -> 4
detector.add_dependency(1, 2);
detector.add_dependency(2, 3);
detector.add_dependency(3, 4);

let result = detector.detect_cycle();
assert!(!result.has_cycle());  // ✅ Valid DAG
```

### Example 2: Simple Cycle

```rust
let mut detector = CycleDetector::new();

// Cycle: 1 -> 2 -> 3 -> 1
detector.add_dependency(1, 2);
detector.add_dependency(2, 3);
detector.add_dependency(3, 1);  // Creates cycle!

let result = detector.detect_cycle();
assert!(result.has_cycle());   // ❌ Cycle detected

println!("Cycle: {:?}", result.cycle_path());
// Output: Cycle: Some([1, 2, 3, 1])
```

### Example 3: Self-Loop

```rust
let mut detector = CycleDetector::new();

// Self-loop: 1 -> 1
detector.add_dependency(1, 1);

let result = detector.detect_cycle();
assert!(result.has_cycle());  // ❌ Self-loop detected
```

### Example 4: Diamond (No Cycle)

```rust
let mut detector = CycleDetector::new();

// Diamond pattern:
//     1
//    / \
//   2   3
//    \ /
//     4
detector.add_dependency(1, 2);
detector.add_dependency(1, 3);
detector.add_dependency(2, 4);
detector.add_dependency(3, 4);

let result = detector.detect_cycle();
assert!(!result.has_cycle());  // ✅ Valid DAG
```

## Topological Sort

Get a valid execution order for tasks:

```rust
use taskflow_rs::CycleDetector;

let mut detector = CycleDetector::new();

detector.add_dependency(1, 2);
detector.add_dependency(2, 3);
detector.add_dependency(1, 3);

// Get topological order
if let Some(sorted) = detector.topological_sort() {
    println!("Execution order: {:?}", sorted);
    // Output: [1, 2, 3]
} else {
    println!("Cannot sort: cycle detected!");
}
```

**Properties:**
- Returns `Some(Vec<TaskId>)` for valid DAGs
- Returns `None` if cycles are detected
- Order respects all dependencies

## Loop Constructs

### Basic Loop Structure

**Note:** The `Loop` construct provides a structural representation of loops. Full runtime loop execution with automatic condition evaluation requires deeper executor integration (planned for future releases). For now, use controlled iteration patterns as shown in the examples.

```rust
use taskflow_rs::{Taskflow, Loop};

let mut taskflow = Taskflow::new();

// Create condition task
let condition = taskflow.emplace(|| {
    // Check condition and log
    println!("Checking loop condition");
    // Note: Return value handling requires executor integration
}).name("loop_condition");

// Create body tasks
let body = taskflow.emplace(|| {
    println!("Loop iteration");
}).name("loop_body");

// Create loop construct (structural representation)
let mut loop_construct = Loop::new(condition);
loop_construct.add_body_task(body);
loop_construct.max_iterations(100);  // Safety limit
```

### Loop API

```rust
pub struct Loop {
    pub condition: TaskHandle,
    pub body: Vec<TaskHandle>,
    pub max_iterations: Option<usize>,
}

impl Loop {
    pub fn new(condition: TaskHandle) -> Self;
    pub fn add_body_task(&mut self, task: TaskHandle) -> &mut Self;
    pub fn max_iterations(&mut self, max: usize) -> &mut Self;
}
```

### Controlled Iteration Example

```rust
use taskflow_rs::{Executor, Taskflow};
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));
let max_iterations = 5;

let mut executor = Executor::new(4);

// Execute iterations
for i in 0..max_iterations {
    let mut taskflow = Taskflow::new();
    let c = counter.clone();
    
    taskflow.emplace(move || {
        let mut count = c.lock().unwrap();
        *count += 1;
        println!("Iteration {}: counter = {}", i + 1, *count);
    });
    
    executor.run(&taskflow).wait();
}

println!("Final count: {}", *counter.lock().unwrap());
```

## Advanced Features

### Strongly Connected Components

Find groups of tasks that form cycles:

```rust
let components = detector.strongly_connected_components();

for (i, component) in components.iter().enumerate() {
    if component.len() > 1 {
        println!("SCC {}: {:?} (contains cycle)", i, component);
    }
}
```

### Integration with Taskflow

Check for cycles before execution:

```rust
use taskflow_rs::{Taskflow, CycleDetector};

let mut taskflow = Taskflow::new();

let a = taskflow.emplace(|| println!("A"));
let b = taskflow.emplace(|| println!("B"));
let c = taskflow.emplace(|| println!("C"));

a.precede(&b);
b.precede(&c);
// Don't do: c.precede(&a);  // Would create cycle!

// Verify no cycles before execution
let mut detector = CycleDetector::new();
detector.add_dependency(a.id, b.id);
detector.add_dependency(b.id, c.id);

if detector.detect_cycle().has_cycle() {
    panic!("Task graph contains cycles!");
}
```

## Use Cases

### 1. Workflow Validation

Validate task graphs before execution:

```rust
fn validate_workflow(tasks: &[(usize, usize)]) -> bool {
    let mut detector = CycleDetector::new();
    
    for &(from, to) in tasks {
        detector.add_dependency(from, to);
    }
    
    !detector.detect_cycle().has_cycle()
}

let valid = validate_workflow(&[(1, 2), (2, 3), (3, 4)]);
assert!(valid);

let invalid = validate_workflow(&[(1, 2), (2, 3), (3, 1)]);
assert!(!invalid);
```

### 2. Iterative Algorithms

Implement algorithms with controlled iteration:

```rust
// Gradient descent
let params = Arc::new(Mutex::new(vec![0.0; 10]));
let max_iters = 1000;
let tolerance = 1e-6;

for iter in 0..max_iters {
    let mut taskflow = Taskflow::new();
    let p = params.clone();
    
    taskflow.emplace(move || {
        let mut params = p.lock().unwrap();
        // Compute gradient and update
        let gradient = compute_gradient(&params);
        update_params(&mut params, &gradient, 0.01);
        
        if gradient_norm(&gradient) < tolerance {
            println!("Converged at iteration {}", iter);
        }
    });
    
    executor.run(&taskflow).wait();
}
```

### 3. Pipeline Processing with Feedback

Process data in loops with feedback:

```rust
let data_queue = Arc::new(Mutex::new(VecDeque::new()));

for round in 0..10 {
    let mut taskflow = Taskflow::new();
    let queue = data_queue.clone();
    
    // Process
    taskflow.emplace(move || {
        let mut q = queue.lock().unwrap();
        if let Some(item) = q.pop_front() {
            let processed = process(item);
            if needs_reprocessing(processed) {
                q.push_back(processed);  // Feedback loop
            }
        }
    });
    
    executor.run(&taskflow).wait();
}
```

## Algorithm Details

### DFS-Based Cycle Detection

```
1. For each unvisited node:
   a. Start DFS from node
   b. Mark node as visited and in current path
   c. For each successor:
      - If not visited: recurse
      - If in current path: cycle detected!
   d. Remove node from current path

Time: O(V + E)
Space: O(V)
```

### Kahn's Algorithm (Topological Sort)

```
1. Calculate in-degree for all nodes
2. Enqueue nodes with in-degree 0
3. While queue not empty:
   a. Dequeue node
   b. Add to sorted list
   c. For each successor:
      - Decrement in-degree
      - If in-degree becomes 0, enqueue
4. If sorted.len() == nodes.len(): success
   Otherwise: cycle exists

Time: O(V + E)
Space: O(V)
```

### Tarjan's Algorithm (SCCs)

```
Uses DFS with additional bookkeeping:
- Index: Discovery time
- Lowlink: Earliest reachable ancestor
- Stack: Current SCC being discovered

Time: O(V + E)
Space: O(V)
```

## Performance

| Operation | Time Complexity | Space Complexity |
|-----------|----------------|------------------|
| Add Task | O(1) | O(1) |
| Add Dependency | O(1) | O(1) |
| Detect Cycle | O(V + E) | O(V) |
| Topological Sort | O(V + E) | O(V) |
| Find SCCs | O(V + E) | O(V) |

Where V = number of tasks, E = number of dependencies

## Best Practices

### 1. Validate Before Execution

```rust
// Always check for cycles before running
if detector.detect_cycle().has_cycle() {
    panic!("Invalid workflow: contains cycles!");
}

executor.run(&taskflow).wait();
```

### 2. Use Safety Limits

```rust
// Always set max iterations for loops
loop_construct.max_iterations(1000);
```

### 3. Clear Error Messages

```rust
match detector.detect_cycle() {
    CycleDetectionResult::NoCycle => {
        println!("✓ Valid DAG");
    }
    CycleDetectionResult::CycleDetected(path) => {
        eprintln!("✗ Cycle detected: {:?}", path);
        eprintln!("Tasks involved: {:?}", path);
        panic!("Cannot execute cyclic graph!");
    }
}
```

### 4. Incremental Validation

```rust
// Check after each dependency addition
detector.add_dependency(a, b);
if detector.detect_cycle().has_cycle() {
    panic!("Adding {} -> {} creates a cycle!", a, b);
}
```

## Limitations

1. **Runtime Loops**: Full runtime loop support requires executor integration
2. **Dynamic Graphs**: Current API assumes static graphs
3. **Conditional Edges**: Branch conditions not tracked in cycle detection

## Future Enhancements

- Automatic cycle detection in executor
- Runtime loop execution support
- Dynamic graph modification
- Cycle breaking suggestions
- Visual cycle highlighting
- Integration with profiler

## Examples

Run the cycle detection demo:

```bash
cargo run --example loop_and_cycle_detection
```

Expected output:
```
=== Cycle Detection and Loop Support Demo ===

1. Cycle Detection
   Test 1: Linear chain (no cycle)
     Graph: 1 -> 2 -> 3 -> 4
     Has cycle: false
     Topological order: [1, 2, 3, 4]

   Test 2: Simple cycle
     Graph: 1 -> 2 -> 3 -> 1
     Has cycle: true
     Cycle path: [1, 2, 3, 1]
     Topological sort: Failed (cycle detected)

   Test 3: Self-loop
     Graph: 1 -> 1
     Has cycle: true

   Test 4: Diamond pattern (no cycle)
     Graph: Diamond (1 branches to 2,3 which merge at 4)
     Has cycle: false

   ✓ Cycle detection working correctly

2. Loop Construct
   ...

3. Controlled Iteration
   ...

4. Complex Graph Analysis
   ...
```

## See Also

- `ConditionalHandle` - Conditional branching
- `BranchId` - Branch identifiers
- `Taskflow` - Task graph construction
