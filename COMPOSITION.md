# Task Graph Composition

TaskFlow-RS provides powerful composition features that allow you to build complex workflows from reusable components.

## Features

- **Taskflow Composition** - Combine multiple taskflows into one
- **Reusable Modules** - Create components that can be instantiated multiple times
- **Entry/Exit Points** - Define clear interfaces for composed components
- **Flexible Connection** - Connect components in series, parallel, or custom patterns

## Core Concepts

### Composition

A `Composition` is a reusable task graph template with defined entry and exit points:

```rust
pub struct Composition {
    // Internal taskflow
    // Entry points (tasks that can receive external dependencies)
    // Exit points (tasks that can provide dependencies to external tasks)
}
```

### CompositionBuilder

Helper for creating compositions with explicit entry/exit points:

```rust
let mut builder = CompositionBuilder::new();

// Build your component
let task_a = builder.taskflow_mut().emplace(|| {
    println!("Task A");
});

let task_b = builder.taskflow_mut().emplace(|| {
    println!("Task B");
});

task_a.precede(&task_b);

// Mark interface points
builder.mark_entries(&[task_a]);
builder.mark_exits(&[task_b]);

let composition = builder.build();
```

### ComposedInstance

Represents an instance of a composition within a target taskflow:

```rust
pub struct ComposedInstance {
    entries: Vec<TaskHandle>,
    exits: Vec<TaskHandle>,
}
```

## Basic Usage

### Creating a Reusable Component

```rust
use taskflow_rs::{CompositionBuilder, Composition};

fn create_data_processor() -> Composition {
    let mut builder = CompositionBuilder::new();
    
    let load = builder.taskflow_mut().emplace(|| {
        println!("Loading data");
    });
    
    let process = builder.taskflow_mut().emplace(|| {
        println!("Processing data");
    });
    
    let save = builder.taskflow_mut().emplace(|| {
        println!("Saving results");
    });
    
    load.precede(&process);
    process.precede(&save);
    
    // Define interface
    builder.mark_entries(&[load]);
    builder.mark_exits(&[save]);
    
    builder.build()
}
```

### Using a Component

```rust
use taskflow_rs::{Taskflow, TaskflowComposable};

let mut main_flow = Taskflow::new();
let processor = create_data_processor();

// Compose it into the main flow
let instance = main_flow.compose(&processor);

// instance.entries() and instance.exits() give you TaskHandles
// that you can connect to other tasks
```

## Composition Patterns

### Pattern 1: Sequential Composition

Chain components in a pipeline:

```rust
let mut main_flow = Taskflow::new();

let stage1 = create_stage("Extract");
let stage2 = create_stage("Transform");
let stage3 = create_stage("Load");

// Compose all stages
let inst1 = main_flow.compose(&stage1);
let inst2 = main_flow.compose(&stage2);
let inst3 = main_flow.compose(&stage3);

// Connect them sequentially
for exit in inst1.exits() {
    for entry in inst2.entries() {
        exit.precede(entry);
    }
}

for exit in inst2.exits() {
    for entry in inst3.entries() {
        exit.precede(entry);
    }
}
```

### Pattern 2: Parallel Composition

Use the same component multiple times:

```rust
let mut main_flow = Taskflow::new();
let worker = create_worker_component();

let start = main_flow.emplace(|| {
    println!("Start");
});

let end = main_flow.emplace(|| {
    println!("End");
});

// Create 4 parallel instances
for _ in 0..4 {
    let instance = main_flow.compose(&worker);
    
    // Connect start to instance
    for entry in instance.entries() {
        start.precede(entry);
    }
    
    // Connect instance to end
    for exit in instance.exits() {
        exit.precede(&end);
    }
}
```

### Pattern 3: Fan-Out/Fan-In

Distribute work and gather results:

```rust
let mut main_flow = Taskflow::new();

let split = main_flow.emplace(|| {
    println!("Split");
});

let merge = main_flow.emplace(|| {
    println!("Merge");
});

// Create components for each branch
let branches = vec![
    create_branch("A"),
    create_branch("B"),
    create_branch("C"),
];

let instances: Vec<_> = branches.iter()
    .map(|b| main_flow.compose(b))
    .collect();

// Fan-out: connect split to all branches
for inst in &instances {
    for entry in inst.entries() {
        split.precede(entry);
    }
}

// Fan-in: connect all branches to merge
for inst in &instances {
    for exit in inst.exits() {
        exit.precede(&merge);
    }
}
```

### Pattern 4: Conditional Composition

Compose different components based on conditions:

```rust
let mut main_flow = Taskflow::new();

let condition = main_flow.emplace_conditional(|| {
    // Return 0 or 1 based on some logic
    if some_condition() { 0 } else { 1 }
});

// Create two different processing pipelines
let fast_path = create_fast_processor();
let slow_path = create_slow_processor();

let fast_instance = main_flow.compose(&fast_path);
let slow_instance = main_flow.compose(&slow_path);

// Branch to different components
condition.branch(0, fast_instance.entries()[0]);
condition.branch(1, slow_instance.entries()[0]);

main_flow.register_branches(&condition);
```

## Advanced Features

### Helper Methods

The `TaskflowComposable` trait provides convenience methods:

```rust
use taskflow_rs::TaskflowComposable;

// Compose with automatic predecessor connection
let instance = main_flow.compose_after(&[predecessor], &component);

// Compose with automatic successor connection
let instance = main_flow.compose_before(&component, &[successor]);
```

### Multiple Entry/Exit Points

Components can have multiple entry and exit points:

```rust
let mut builder = CompositionBuilder::new();

let entry_a = builder.taskflow_mut().emplace(|| {});
let entry_b = builder.taskflow_mut().emplace(|| {});

let exit_x = builder.taskflow_mut().emplace(|| {});
let exit_y = builder.taskflow_mut().emplace(|| {});

// Mark multiple interface points
builder.mark_entries(&[entry_a, entry_b]);
builder.mark_exits(&[exit_x, exit_y]);

let comp = builder.build();

// Later, connect specific points
let instance = main_flow.compose(&comp);
some_task.precede(instance.entry(0).unwrap()); // Connect to entry_a
other_task.precede(instance.entry(1).unwrap()); // Connect to entry_b
```

## Complete Example

Here's a complete example building a data processing pipeline from reusable components:

```rust
use taskflow_rs::{
    Executor, Taskflow, Composition, CompositionBuilder, TaskflowComposable
};

fn main() {
    let mut executor = Executor::new(4);
    
    // Create reusable components
    let ingestion = create_ingestion_component();
    let validation = create_validation_component();
    let enrichment = create_enrichment_component();
    let analytics = create_analytics_component();
    let export = create_export_component();
    
    // Compose them into a pipeline
    let mut pipeline = Taskflow::new();
    
    let ing_inst = pipeline.compose(&ingestion);
    let val_inst = pipeline.compose(&validation);
    let enr_inst = pipeline.compose(&enrichment);
    let ana_inst = pipeline.compose(&analytics);
    let exp_inst = pipeline.compose(&export);
    
    // Connect in sequence
    connect_sequential(&ing_inst, &val_inst);
    connect_sequential(&val_inst, &enr_inst);
    connect_sequential(&enr_inst, &ana_inst);
    connect_sequential(&ana_inst, &exp_inst);
    
    // Execute
    executor.run(&pipeline).wait();
}

fn connect_sequential(from: &ComposedInstance, to: &ComposedInstance) {
    for exit in from.exits() {
        for entry in to.entries() {
            exit.precede(entry);
        }
    }
}

fn create_ingestion_component() -> Composition {
    let mut builder = CompositionBuilder::new();
    
    let fetch = builder.taskflow_mut().emplace(|| {
        println!("Fetching data from sources");
    });
    
    let parse = builder.taskflow_mut().emplace(|| {
        println!("Parsing raw data");
    });
    
    fetch.precede(&parse);
    
    builder.mark_entries(&[fetch]);
    builder.mark_exits(&[parse]);
    builder.build()
}

fn create_validation_component() -> Composition {
    let mut builder = CompositionBuilder::new();
    
    let validate = builder.taskflow_mut().emplace(|| {
        println!("Validating data schema");
    });
    
    let sanitize = builder.taskflow_mut().emplace(|| {
        println!("Sanitizing data");
    });
    
    validate.precede(&sanitize);
    
    builder.mark_entries(&[validate]);
    builder.mark_exits(&[sanitize]);
    builder.build()
}

// ... implement other components similarly
```

## API Reference

### Composition

```rust
impl Composition {
    pub fn new(taskflow: Taskflow, entries: Vec<TaskHandle>, exits: Vec<TaskHandle>) -> Self;
    pub fn entries(&self) -> &[TaskHandle];
    pub fn exits(&self) -> &[TaskHandle];
    pub fn compose_into(&self, target: &mut Taskflow) -> ComposedInstance;
    pub fn taskflow(&self) -> &Taskflow;
}
```

### CompositionBuilder

```rust
impl CompositionBuilder {
    pub fn new() -> Self;
    pub fn taskflow_mut(&mut self) -> &mut Taskflow;
    pub fn mark_entries(&mut self, tasks: &[TaskHandle]);
    pub fn mark_exits(&mut self, tasks: &[TaskHandle]);
    pub fn build(self) -> Composition;
}
```

### ComposedInstance

```rust
impl ComposedInstance {
    pub fn entries(&self) -> &[TaskHandle];
    pub fn exits(&self) -> &[TaskHandle];
    pub fn entry(&self, index: usize) -> Option<&TaskHandle>;
    pub fn exit(&self, index: usize) -> Option<&TaskHandle>;
}
```

### TaskflowComposable

```rust
pub trait TaskflowComposable {
    fn compose(&mut self, other: &Composition) -> ComposedInstance;
    fn compose_after(&mut self, predecessors: &[TaskHandle], other: &Composition) -> ComposedInstance;
    fn compose_before(&mut self, other: &Composition, successors: &[TaskHandle]) -> ComposedInstance;
}
```

## Best Practices

### 1. Keep Components Focused

Each component should do one thing well:

```rust
// Good: Focused component
fn create_validator() -> Composition {
    // Just validation logic
}

// Less good: Component does too much
fn create_processor_and_validator_and_exporter() -> Composition {
    // Too many responsibilities
}
```

### 2. Define Clear Interfaces

Always mark entry and exit points explicitly:

```rust
builder.mark_entries(&[start_task]);
builder.mark_exits(&[end_task]);
```

### 3. Name Your Tasks

Named tasks make debugging easier:

```rust
let task = builder.taskflow_mut()
    .emplace(|| { /* work */ })
    .name("data_validator");
```

### 4. Document Component Behavior

```rust
/// Creates a data validation component.
/// 
/// Entry points: Single validator task
/// Exit points: Single completion task
/// 
/// This component validates data against a schema and
/// filters out invalid entries.
fn create_validator() -> Composition {
    // ...
}
```

## Limitations

1. **Work Cloning**: Currently, task work cannot be cloned between compositions (uses placeholders)
2. **No Dynamic Modification**: Once composed, components cannot be modified
3. **Linear ID Space**: Task IDs must be unique across all compositions

## Future Enhancements

- Parameterized compositions (pass configuration)
- Work cloning support for full composition
- Component versioning and libraries
- Visual composition tools
- Composition validation and type checking

## Examples

Run the composition examples:

```bash
cargo run --example composition
```

This demonstrates:
- Simple composition
- Reusable modules
- Sequential pipelines
- Parallel fan-out/fan-in patterns
