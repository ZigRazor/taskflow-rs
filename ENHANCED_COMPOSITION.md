# Enhanced Composition

TaskFlow-RS now features complete composition support with work cloning and parameterization.

## Features

- **Cloneable Work** - True work reusability through `Fn()` closures
- **Parameterized Compositions** - Dynamic graph generation based on parameters
- **Type-Safe Parameters** - Int, Float, String, Bool parameter types
- **Reusable Patterns** - Build once, use many times with different configurations

## Cloneable Work

### Problem with Original Composition

The original composition had a limitation: task work uses `FnOnce()` closures which can't be cloned. This meant compositions were placeholders.

### Solution: CloneableWork

Use `Fn()` closures wrapped in `Arc` for true cloning:

```rust
use taskflow_rs::CompositionBuilder;
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));

let mut builder = CompositionBuilder::new();

let c = counter.clone();
let task = builder.emplace_cloneable(move || {
    *c.lock().unwrap() += 1;
    println!("Counter: {}", *c.lock().unwrap());
});

let composition = builder.build();

// Use multiple times - work is truly cloned!
taskflow1.compose(&composition);
taskflow2.compose(&composition);
```

### API

```rust
impl CompositionBuilder {
    /// Add a cloneable task (uses Fn instead of FnOnce)
    pub fn emplace_cloneable<F>(&mut self, work: F) -> TaskHandle
    where F: Fn() + Send + Sync + 'static;
}
```

**Key Difference:**
- `emplace()` - FnOnce (can't clone, placeholder on reuse)
- `emplace_cloneable()` - Fn (clones properly, truly reusable)

## Parameterized Compositions

Create compositions that adapt based on parameters:

### Basic Example

```rust
use taskflow_rs::{ParameterizedComposition, CompositionParams, CompositionBuilder};

let param_comp = ParameterizedComposition::new(|params| {
    let mut builder = CompositionBuilder::new();
    
    // Get parameters
    let num_tasks = params.get_int("num_tasks").unwrap_or(3);
    let message = params.get_string("message").unwrap_or("Task");
    
    // Build composition based on parameters
    let mut tasks = Vec::new();
    for i in 0..num_tasks {
        let msg = message.to_string();
        let task = builder.emplace_cloneable(move || {
            println!("{} {}", msg, i);
        });
        tasks.push(task);
    }
    
    // Chain tasks
    for i in 0..tasks.len() - 1 {
        tasks[i].precede(&tasks[i + 1]);
    }
    
    if !tasks.is_empty() {
        builder.mark_entries(&[tasks[0].clone()]);
        builder.mark_exits(&[tasks.last().unwrap().clone()]);
    }
    
    builder.build()
});

// Use with different parameters
let mut params1 = CompositionParams::new();
params1.set_int("num_tasks", 5);
params1.set_string("message", "Step".to_string());

param_comp.compose_into(&mut taskflow, &params1);
```

### Parameter Types

```rust
impl CompositionParams {
    // Integer parameters
    pub fn set_int(&mut self, key: &str, value: i64) -> &mut Self;
    pub fn get_int(&self, key: &str) -> Option<i64>;
    
    // Float parameters
    pub fn set_float(&mut self, key: &str, value: f64) -> &mut Self;
    pub fn get_float(&self, key: &str) -> Option<f64>;
    
    // String parameters
    pub fn set_string(&mut self, key: &str, value: String) -> &mut Self;
    pub fn get_string(&self, key: &str) -> Option<&str>;
    
    // Boolean parameters
    pub fn set_bool(&mut self, key: &str, value: bool) -> &mut Self;
    pub fn get_bool(&self, key: &str) -> Option<bool>;
}
```

### Default Parameters

```rust
let mut defaults = CompositionParams::new();
defaults.set_int("workers", 4);
defaults.set_bool("verbose", true);

let param_comp = ParameterizedComposition::new(factory)
    .with_defaults(defaults);

// Use defaults
let comp = param_comp.instantiate_default();
```

## Common Patterns

### 1. Map-Reduce Pattern

```rust
let map_reduce = ParameterizedComposition::new(|params| {
    let mut builder = CompositionBuilder::new();
    
    let num_mappers = params.get_int("mappers").unwrap_or(4) as usize;
    
    // Map phase
    let mut mappers = Vec::new();
    for i in 0..num_mappers {
        let mapper = builder.emplace_cloneable(move || {
            println!("Map {}", i);
        });
        mappers.push(mapper);
    }
    
    // Reduce phase
    let reducer = builder.emplace_cloneable(|| {
        println!("Reduce");
    });
    
    for mapper in &mappers {
        mapper.precede(&reducer);
    }
    
    builder.mark_entries(&mappers);
    builder.mark_exits(&[reducer]);
    builder.build()
});
```

### 2. Pipeline Pattern

```rust
let pipeline = ParameterizedComposition::new(|params| {
    let mut builder = CompositionBuilder::new();
    
    let num_stages = params.get_int("stages").unwrap_or(3) as usize;
    
    let mut stages = Vec::new();
    for i in 0..num_stages {
        let stage = builder.emplace_cloneable(move || {
            println!("Stage {}", i);
        });
        stages.push(stage);
    }
    
    // Chain stages
    for i in 0..stages.len() - 1 {
        stages[i].precede(&stages[i + 1]);
    }
    
    if !stages.is_empty() {
        builder.mark_entries(&[stages[0].clone()]);
        builder.mark_exits(&[stages.last().unwrap().clone()]);
    }
    
    builder.build()
});
```

### 3. Fork-Join Pattern

```rust
let fork_join = ParameterizedComposition::new(|params| {
    let mut builder = CompositionBuilder::new();
    
    let num_workers = params.get_int("workers").unwrap_or(4) as usize;
    
    // Fork
    let fork = builder.emplace_cloneable(|| {
        println!("Fork");
    });
    
    // Workers
    let mut workers = Vec::new();
    for i in 0..num_workers {
        let worker = builder.emplace_cloneable(move || {
            println!("Worker {}", i);
        });
        fork.precede(&worker);
        workers.push(worker);
    }
    
    // Join
    let join = builder.emplace_cloneable(|| {
        println!("Join");
    });
    
    for worker in &workers {
        worker.precede(&join);
    }
    
    builder.mark_entries(&[fork]);
    builder.mark_exits(&[join]);
    builder.build()
});
```

## Complex Example: Multi-Stage Pipeline

```rust
use taskflow_rs::{Executor, Taskflow, ParameterizedComposition, CompositionParams};

// Stage 1: Preprocessing
let preprocess = ParameterizedComposition::new(|_params| {
    let mut builder = CompositionBuilder::new();
    
    let validate = builder.emplace_cloneable(|| println!("Validate"));
    let clean = builder.emplace_cloneable(|| println!("Clean"));
    
    validate.precede(&clean);
    
    builder.mark_entries(&[validate]);
    builder.mark_exits(&[clean]);
    builder.build()
});

// Stage 2: Processing
let process = ParameterizedComposition::new(|params| {
    let mut builder = CompositionBuilder::new();
    
    let workers = params.get_int("workers").unwrap_or(4) as usize;
    
    let mut tasks = Vec::new();
    for i in 0..workers {
        let task = builder.emplace_cloneable(move || {
            println!("Process {}", i);
        });
        tasks.push(task);
    }
    
    builder.mark_entries(&tasks);
    builder.mark_exits(&tasks);
    builder.build()
});

// Stage 3: Postprocessing
let postprocess = ParameterizedComposition::new(|_params| {
    let mut builder = CompositionBuilder::new();
    
    let aggregate = builder.emplace_cloneable(|| println!("Aggregate"));
    let save = builder.emplace_cloneable(|| println!("Save"));
    
    aggregate.precede(&save);
    
    builder.mark_entries(&[aggregate]);
    builder.mark_exits(&[save]);
    builder.build()
});

// Assemble pipeline
let mut executor = Executor::new(4);
let mut taskflow = Taskflow::new();

let params = CompositionParams::new();
let mut process_params = CompositionParams::new();
process_params.set_int("workers", 8);

let pre = preprocess.compose_into(&mut taskflow, &params);
let proc = process.compose_into(&mut taskflow, &process_params);
let post = postprocess.compose_into(&mut taskflow, &params);

// Connect stages
for exit in pre.exits() {
    for entry in proc.entries() {
        exit.precede(entry);
    }
}

for exit in proc.exits() {
    for entry in post.entries() {
        exit.precede(entry);
    }
}

executor.run(&taskflow).wait();
```

## Best Practices

### 1. Use Cloneable Work for Reusable Components

```rust
// ✓ Good - truly reusable
builder.emplace_cloneable(|| { /* work */ });

// ✗ Bad - placeholder on reuse
builder.taskflow_mut().emplace(|| { /* work */ });
```

### 2. Validate Parameters

```rust
let param_comp = ParameterizedComposition::new(|params| {
    let workers = params.get_int("workers").unwrap_or(4);
    
    // Validate
    let workers = workers.max(1).min(16);
    
    // Use validated value
    // ...
});
```

### 3. Provide Meaningful Defaults

```rust
let num_tasks = params.get_int("num_tasks").unwrap_or(4);
let timeout = params.get_int("timeout_ms").unwrap_or(1000);
let verbose = params.get_bool("verbose").unwrap_or(false);
```

### 4. Document Expected Parameters

```rust
/// Creates a parallel processing composition
/// 
/// Parameters:
/// - "workers" (int): Number of parallel workers (default: 4)
/// - "batch_size" (int): Batch size for processing (default: 100)
/// - "verbose" (bool): Enable verbose logging (default: false)
let parallel_processor = ParameterizedComposition::new(|params| {
    // ...
});
```

## Performance Considerations

### Work Cloning Overhead

- `emplace_cloneable()` uses `Arc<dyn Fn()>` - minimal overhead
- Single allocation shared across all instances
- No performance impact at execution time

### Parameter Overhead

- Parameters parsed at composition time, not execution time
- Zero runtime overhead after instantiation
- Consider caching instantiated compositions if used frequently

## Migration Guide

### From Old Composition

```rust
// Before (placeholder work on reuse)
let task = builder.taskflow_mut().emplace(|| {
    println!("Work");
});

// After (truly cloneable)
let task = builder.emplace_cloneable(|| {
    println!("Work");
});
```

### Adding Parameters

```rust
// Before (fixed structure)
fn create_pipeline() -> Composition {
    let mut builder = CompositionBuilder::new();
    // Fixed 3 stages
    // ...
    builder.build()
}

// After (configurable structure)
let pipeline = ParameterizedComposition::new(|params| {
    let mut builder = CompositionBuilder::new();
    let num_stages = params.get_int("stages").unwrap_or(3);
    // Dynamic stages based on parameter
    // ...
    builder.build()
});
```

## Examples

Run the enhanced composition demo:

```bash
cargo run --example enhanced_composition
```

Expected output:
```
=== Enhanced Composition Demo ===

1. Cloneable Work
   Compositions with properly cloned work

   First execution:
   Task 1: Count = 1
   Task 2: Count = 11

   Second execution:
   Task 1: Count = 12
   Task 2: Count = 22

   Final counter: 22
   ✓ Work was properly cloned and reused

2. Parameterized Composition
   ...

3. Reusable Patterns
   ...

4. Complex Pipeline
   ...
```

## See Also

- `Composition` - Basic composition
- `CompositionBuilder` - Build compositions
- `TaskflowComposable` - Composition trait
- [COMPOSITION.md](COMPOSITION.md) - Original composition docs
