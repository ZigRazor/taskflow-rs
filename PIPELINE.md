# Pipeline Support

TaskFlow-RS provides powerful stream processing capabilities with concurrent pipelines, token management, and automatic backpressure handling.

## Features

- **Stream Processing**: Process data streams with multiple stages
- **Parallel & Serial Stages**: Mix serial and parallel processing stages
- **Token Management**: Track data items with unique tokens
- **Backpressure Handling**: Automatic flow control to prevent memory overflow
- **Thread-Safe**: Built with Arc/Mutex for concurrent access
- **Producer-Consumer Pattern**: Easy multi-threaded data processing

## Core Concepts

### Token

A `Token<T>` wraps data flowing through the pipeline:

```rust
pub struct Token<T> {
    pub data: T,      // The actual data
    pub index: usize, // Unique token identifier
}
```

### ConcurrentPipeline

The main pipeline structure:

```rust
pub struct ConcurrentPipeline<T> {
    // Internal queues, synchronization primitives
}

impl<T> ConcurrentPipeline<T> {
    pub fn new(buffer_size: usize, max_tokens: usize) -> Self;
    pub fn push(&self, data: T) -> Result<(), &'static str>;
    pub fn try_pop(&self) -> Option<Token<T>>;
    pub fn stop(&self);
    pub fn tokens_in_flight(&self) -> usize;
}
```

**Parameters:**
- `buffer_size`: Maximum items in internal queue (backpressure threshold)
- `max_tokens`: Maximum total tokens in flight across entire pipeline

## Basic Usage

### Simple Pipeline

```rust
// Option 1: Use re-exports from crate root
use taskflow_rs::{ConcurrentPipeline, Token};

// Option 2: Use full module path
use taskflow_rs::pipeline::{ConcurrentPipeline, Token};

let pipeline = ConcurrentPipeline::new(10, 100);

// Push data
pipeline.push(42).unwrap();

// Pop processed data
if let Some(token) = pipeline.try_pop() {
    println!("Token #{}: {}", token.index, token.data);
}
```

### Producer-Consumer Pattern

```rust
use std::thread;

let pipeline = ConcurrentPipeline::new(10, 50);
let p_clone = pipeline.clone();

// Producer thread
let producer = thread::spawn(move || {
    for i in 0..100 {
        p_clone.push(i).unwrap();
    }
    p_clone.stop();
});

// Consumer thread
let c_clone = pipeline.clone();
let consumer = thread::spawn(move || {
    while !c_clone.is_stopped() {
        if let Some(token) = c_clone.try_pop() {
            println!("Processed: {}", token.data);
        }
    }
});

producer.join().unwrap();
consumer.join().unwrap();
```

## Multi-Stage Pipelines

### Serial Stages

Process one item at a time:

```rust
// Stage 1: Input → Parse
let input = ConcurrentPipeline::new(10, 100);
let parsed = ConcurrentPipeline::new(10, 100);

let stage1 = thread::spawn(move || {
    while let Some(token) = input.try_pop() {
        let result = parse(token.data);
        parsed.push(result).ok();
    }
});
```

### Parallel Stages

Multiple workers processing concurrently:

```rust
let input = ConcurrentPipeline::new(20, 100);
let output = ConcurrentPipeline::new(20, 100);

let num_workers = 4;
for worker_id in 0..num_workers {
    let i = input.clone();
    let o = output.clone();
    
    thread::spawn(move || {
        while let Some(token) = i.try_pop() {
            let result = expensive_computation(token.data);
            o.push(result).ok();
        }
    });
}
```

## Backpressure Handling

### Automatic Flow Control

When the buffer is full, `push()` returns an error:

```rust
let pipeline = ConcurrentPipeline::new(3, 10);

// Fill buffer
pipeline.push(1).unwrap();
pipeline.push(2).unwrap();
pipeline.push(3).unwrap();

// Buffer full - returns Err
match pipeline.push(4) {
    Ok(_) => println!("Success"),
    Err(e) => println!("Backpressure: {}", e),
}

// Pop one to make space
pipeline.try_pop();

// Now push succeeds
pipeline.push(4).unwrap();
```

### Manual Backpressure

Check tokens in flight:

```rust
if pipeline.tokens_in_flight() < pipeline.max_tokens {
    pipeline.push(data).ok();
} else {
    // Wait or drop data
    thread::sleep(Duration::from_millis(10));
}
```

## Complete Example: Data Processing Pipeline

```rust
use taskflow_rs::ConcurrentPipeline;
use std::thread;
use std::time::Duration;

fn main() {
    // Create pipeline stages
    let raw_input = ConcurrentPipeline::new(10, 100);
    let parsed = ConcurrentPipeline::new(10, 100);
    let transformed = ConcurrentPipeline::new(10, 100);
    let validated = ConcurrentPipeline::new(10, 100);
    
    // Stage 1: Parse (serial)
    let stage1 = {
        let input = raw_input.clone();
        let output = parsed.clone();
        thread::spawn(move || {
            while !input.is_stopped() {
                if let Some(token) = input.try_pop() {
                    let parsed_data = parse_input(token.data);
                    output.push(parsed_data).ok();
                }
            }
            output.stop();
        })
    };
    
    // Stage 2: Transform (parallel - 3 workers)
    let mut stage2_workers = vec![];
    for worker_id in 0..3 {
        let input = parsed.clone();
        let output = transformed.clone();
        
        let worker = thread::spawn(move || {
            while !input.is_stopped() {
                if let Some(token) = input.try_pop() {
                    // Heavy processing
                    let result = transform_data(token.data);
                    output.push(result).ok();
                }
            }
        });
        stage2_workers.push(worker);
    }
    
    // Stage 3: Validate (serial)
    let stage3 = {
        let input = transformed.clone();
        let output = validated.clone();
        thread::spawn(move || {
            while !input.is_stopped() {
                if let Some(token) = input.try_pop() {
                    if validate(token.data) {
                        output.push(token.data).ok();
                    }
                }
            }
            output.stop();
        })
    };
    
    // Producer
    let producer = thread::spawn(move || {
        for i in 0..100 {
            raw_input.push(i).unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        raw_input.stop();
    });
    
    // Consumer
    let consumer = thread::spawn(move || {
        let mut results = vec![];
        while !validated.is_stopped() || validated.tokens_in_flight() > 0 {
            if let Some(token) = validated.try_pop() {
                results.push(token.data);
            }
        }
        println!("Processed {} items", results.len());
    });
    
    // Wait for completion
    producer.join().unwrap();
    stage1.join().unwrap();
    for worker in stage2_workers {
        worker.join().unwrap();
    }
    transformed.stop(); // Signal stage 2 complete
    stage3.join().unwrap();
    consumer.join().unwrap();
}

fn parse_input(data: i32) -> i32 { data }
fn transform_data(data: i32) -> i32 { data * 2 }
fn validate(data: i32) -> bool { data < 1000 }
```

## Performance Considerations

### Buffer Sizing

- **Small buffers (5-20)**: Better memory usage, more backpressure events
- **Large buffers (50-200)**: Smoother flow, higher memory usage
- **Rule of thumb**: Buffer size ≈ (producer rate / consumer rate) * latency

### Token Limits

Set `max_tokens` based on:
- Available memory
- Total pipeline depth
- Expected throughput

### Worker Count

For parallel stages:
- CPU-bound: `num_workers = num_cpus`
- I/O-bound: `num_workers = num_cpus * 2-4`
- Mixed: Benchmark to find optimal count

## API Reference

### ConcurrentPipeline

```rust
impl<T: Send + Clone + 'static> ConcurrentPipeline<T> {
    /// Create new pipeline
    pub fn new(buffer_size: usize, max_tokens: usize) -> Self;
    
    /// Push data (blocks if buffer full)
    pub fn push(&self, data: T) -> Result<(), &'static str>;
    
    /// Try to pop data (non-blocking)
    pub fn try_pop(&self) -> Option<Token<T>>;
    
    /// Stop the pipeline
    pub fn stop(&self);
    
    /// Check if stopped
    pub fn is_stopped(&self) -> bool;
    
    /// Get current tokens in flight
    pub fn tokens_in_flight(&self) -> usize;
}
```

### Token

```rust
pub struct Token<T> {
    pub data: T,
    pub index: usize,
}

impl<T> Token<T> {
    pub fn new(data: T, index: usize) -> Self;
}
```

## Examples

Run the examples:

```bash
# Basic pipeline with backpressure
cargo run --example pipeline_basic

# Advanced multi-stage processing
cargo run --example pipeline_advanced
```

## Common Patterns

### Pattern 1: Fan-Out (One producer, multiple consumers)

```rust
let input = ConcurrentPipeline::new(20, 100);

for _ in 0..4 {
    let i = input.clone();
    thread::spawn(move || {
        while let Some(token) = i.try_pop() {
            process(token.data);
        }
    });
}
```

### Pattern 2: Fan-In (Multiple producers, one consumer)

```rust
let output = ConcurrentPipeline::new(20, 100);

for i in 0..4 {
    let o = output.clone();
    thread::spawn(move || {
        for data in generate_data(i) {
            o.push(data).ok();
        }
    });
}
```

### Pattern 3: Pipeline with Filtering

```rust
while let Some(token) = input.try_pop() {
    if should_process(token.data) {
        let result = process(token.data);
        output.push(result).ok();
    }
    // Filtered items are dropped
}
```

## Limitations

1. **No Guaranteed Ordering**: Parallel stages may reorder tokens
2. **Manual Stage Management**: No automatic stage lifecycle
3. **Error Handling**: Currently panics on push failures (retry logic needed)
4. **No Built-in Metrics**: Add custom counters for monitoring

## Future Enhancements

- Type-safe pipeline builder with compile-time stage validation
- Built-in retry and error handling
- Performance metrics and monitoring
- Integration with async/await
- GPU pipeline stages
- Automatic stage scaling based on backpressure
