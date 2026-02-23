# Testing Pipeline Support

This guide covers testing strategies for pipeline functionality in TaskFlow-RS.

## Unit Tests

Run built-in pipeline tests:

```bash
cargo test pipeline
```

### Basic Functionality Tests

```rust
use taskflow_rs::{ConcurrentPipeline, Token};

#[test]
fn test_push_pop() {
    let pipeline = ConcurrentPipeline::new(10, 100);
    
    pipeline.push(42).unwrap();
    assert_eq!(pipeline.tokens_in_flight(), 1);
    
    let token = pipeline.try_pop().unwrap();
    assert_eq!(token.data, 42);
    assert_eq!(pipeline.tokens_in_flight(), 0);
}

#[test]
fn test_token_indexing() {
    let pipeline = ConcurrentPipeline::new(10, 100);
    
    pipeline.push(1).unwrap();
    pipeline.push(2).unwrap();
    
    let t1 = pipeline.try_pop().unwrap();
    let t2 = pipeline.try_pop().unwrap();
    
    assert_eq!(t1.index, 0);
    assert_eq!(t2.index, 1);
}

#[test]
fn test_backpressure() {
    let pipeline = ConcurrentPipeline::new(2, 10);
    
    assert!(pipeline.push(1).is_ok());
    assert!(pipeline.push(2).is_ok());
    
    // Buffer full
    assert!(pipeline.push(3).is_err());
    
    // Make space
    pipeline.try_pop();
    
    // Should work now
    assert!(pipeline.push(3).is_ok());
}

#[test]
fn test_stop_signal() {
    let pipeline = ConcurrentPipeline::new(10, 100);
    
    assert!(!pipeline.is_stopped());
    pipeline.stop();
    assert!(pipeline.is_stopped());
}
```

## Integration Tests

### Producer-Consumer Test

```rust
use std::thread;
use std::time::Duration;

#[test]
fn test_producer_consumer() {
    let pipeline = ConcurrentPipeline::new(10, 100);
    let num_items = 100;
    
    let p = pipeline.clone();
    let producer = thread::spawn(move || {
        for i in 0..num_items {
            p.push(i).unwrap();
        }
        p.stop();
    });
    
    let c = pipeline.clone();
    let consumer = thread::spawn(move || {
        let mut received = 0;
        while !c.is_stopped() || c.tokens_in_flight() > 0 {
            if let Some(_) = c.try_pop() {
                received += 1;
            }
        }
        received
    });
    
    producer.join().unwrap();
    let received = consumer.join().unwrap();
    
    assert_eq!(received, num_items);
}
```

### Multi-Stage Pipeline Test

```rust
#[test]
fn test_multi_stage() {
    let stage1_out = ConcurrentPipeline::new(10, 100);
    let stage2_out = ConcurrentPipeline::new(10, 100);
    
    // Stage 1: Double the value
    let s1_in = stage1_out.clone();
    let s1_out = stage1_out.clone();
    let stage1 = thread::spawn(move || {
        for i in 0..10 {
            s1_in.push(i).unwrap();
        }
        s1_in.stop();
    });
    
    // Stage 2: Square the value
    let s2_in = stage1_out.clone();
    let s2_out = stage2_out.clone();
    let stage2 = thread::spawn(move || {
        while !s2_in.is_stopped() || s2_in.tokens_in_flight() > 0 {
            if let Some(token) = s2_in.try_pop() {
                let doubled = token.data * 2;
                s2_out.push(doubled * doubled).ok();
            }
        }
        s2_out.stop();
    });
    
    // Verify
    let mut results = vec![];
    while !stage2_out.is_stopped() || stage2_out.tokens_in_flight() > 0 {
        if let Some(token) = stage2_out.try_pop() {
            results.push(token.data);
        }
    }
    
    stage1.join().unwrap();
    stage2.join().unwrap();
    
    // 0→0, 1→4, 2→16, 3→36, etc.
    assert_eq!(results[0], 0);
    assert_eq!(results[1], 4);
    assert_eq!(results[2], 16);
}
```

## Running Examples

### Basic Pipeline

```bash
cargo run --example pipeline_basic
```

Expected output:
```
=== Pipeline Stream Processing Demo ===

1. Basic Pipeline
   Simple data flow with tokens

   Pushed: 0, Tokens in flight: 1
   Pushed: 10, Tokens in flight: 2
   ...
   Popped token #0: data = 0
   Popped token #1: data = 10
   ✓ Pipeline processed all tokens
```

### Advanced Pipeline

```bash
cargo run --example pipeline_advanced
```

Expected output shows multi-stage processing with parallel workers.

### Benchmarks

```bash
cargo run --release --example pipeline_benchmark
```

Expected output:
```
=== Pipeline Benchmarks ===

1. Throughput Benchmark
   Processed: 10000 items
   Time: 123ms
   Throughput: 81300.81 items/sec

2. Parallel Scaling Benchmark
   1 worker(s): 9615.38 items/sec
   2 worker(s): 18518.52 items/sec
   4 worker(s): 35714.29 items/sec
   8 worker(s): 50000.00 items/sec
```

## Performance Testing

### Throughput Test

Measure items/second:

```rust
let num_items = 100_000;
let start = Instant::now();

// ... run pipeline ...

let elapsed = start.elapsed();
let throughput = num_items as f64 / elapsed.as_secs_f64();
println!("Throughput: {:.2} items/sec", throughput);
```

### Latency Test

Measure end-to-end latency:

```rust
use std::time::Instant;

let pipeline = ConcurrentPipeline::new(10, 100);

let start = Instant::now();
pipeline.push(42).unwrap();
let token = pipeline.try_pop().unwrap();
let latency = start.elapsed();

println!("Latency: {:?}", latency);
```

### Memory Test

Monitor memory usage:

```bash
# Linux
/usr/bin/time -v cargo run --release --example pipeline_benchmark

# macOS
/usr/bin/time -l cargo run --release --example pipeline_benchmark
```

## Stress Testing

### High-Load Test

```rust
#[test]
#[ignore] // Run with: cargo test --ignored
fn stress_test_high_load() {
    let pipeline = ConcurrentPipeline::new(1000, 1_000_000);
    
    let num_producers = 8;
    let items_per_producer = 100_000;
    
    let mut producers = vec![];
    for _ in 0..num_producers {
        let p = pipeline.clone();
        producers.push(thread::spawn(move || {
            for i in 0..items_per_producer {
                p.push(i).unwrap();
            }
        }));
    }
    
    // ... verify all items processed ...
}
```

### Backpressure Stress

```rust
#[test]
fn stress_test_backpressure() {
    let pipeline = ConcurrentPipeline::new(5, 100);
    
    // Fast producer, slow consumer
    let p = pipeline.clone();
    let producer = thread::spawn(move || {
        let mut failures = 0;
        for i in 0..1000 {
            if p.push(i).is_err() {
                failures += 1;
            }
        }
        failures
    });
    
    thread::sleep(Duration::from_millis(100));
    
    let c = pipeline.clone();
    let consumer = thread::spawn(move || {
        while c.tokens_in_flight() > 0 {
            c.try_pop();
            thread::sleep(Duration::from_millis(1));
        }
    });
    
    let failures = producer.join().unwrap();
    consumer.join().unwrap();
    
    assert!(failures > 0, "Expected backpressure events");
}
```

## Debugging Tips

### Enable Logging

Add logging to track pipeline behavior:

```rust
if let Some(token) = pipeline.try_pop() {
    println!("[{}] Processed token #{}: {}", 
             thread::current().name().unwrap_or("?"),
             token.index, 
             token.data);
}
```

### Monitor Tokens

Track tokens in flight:

```rust
loop {
    let in_flight = pipeline.tokens_in_flight();
    if in_flight > threshold {
        println!("Warning: {} tokens in flight", in_flight);
    }
    thread::sleep(Duration::from_millis(100));
}
```

### Deadlock Detection

If pipeline hangs:

1. Check if all threads are joined
2. Verify stop() is called
3. Ensure no circular dependencies between stages
4. Check for missing try_pop() calls

## Common Issues

### Issue 1: Pipeline Hangs

**Symptom**: Consumer never finishes  
**Cause**: Forgot to call `stop()`  
**Fix**: Always call `pipeline.stop()` after last push

### Issue 2: Backpressure Errors

**Symptom**: `push()` returns `Err`  
**Cause**: Buffer full, consumer too slow  
**Fix**: Increase buffer size or add retry logic

### Issue 3: Token Loss

**Symptom**: Fewer items out than in  
**Cause**: Not waiting for tokens in flight  
**Fix**: Check `tokens_in_flight()` before exiting

```rust
// Wrong
while !pipeline.is_stopped() {
    pipeline.try_pop();
}

// Right
while !pipeline.is_stopped() || pipeline.tokens_in_flight() > 0 {
    pipeline.try_pop();
}
```

## Next Steps

After verifying basic pipeline functionality:

1. Test with real workloads
2. Benchmark against requirements
3. Tune buffer sizes
4. Add monitoring/metrics
5. Consider type-safe pipeline builder for complex flows
