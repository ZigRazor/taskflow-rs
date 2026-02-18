# Benchmarking Parallel Systems - Lessons Learned

## Why the Original Benchmark Didn't Show Speedup

Your benchmark results showed:
```
1. Wide Graph (1000 independent tasks)
  1 workers: 1.252ms
  2 workers: 1.246ms  (no improvement!)
  4 workers: 1.419ms  (slower!)
  8 workers: 987µs    (slight improvement)
```

This is actually **correct behavior** - not a bug! Here's why:

### Problem: Tasks Were Too Lightweight

The original tasks looked like this:
```rust
taskflow.emplace(move || {
    let mut sum = 0u64;
    for j in 0..1000 {
        sum = sum.wrapping_add((i * j) as u64);
    }
    *counter.lock().unwrap() += sum;
});
```

**Time breakdown per task:**
- Actual computation: ~10-50 nanoseconds
- Work-stealing overhead: ~500-1000 nanoseconds
- Lock acquisition: ~50-200 nanoseconds
- Thread synchronization: ~100-500 nanoseconds

**Result:** Overhead >> Actual Work

### Amdahl's Law in Action

With tiny tasks:
- **Serial portion:** Task setup, dependency resolution, cleanup (~80%)
- **Parallel portion:** Actual task execution (~20%)

Maximum speedup = 1 / (0.8 + 0.2/N) ≈ 1.25x even with infinite cores!

## How to Properly Benchmark Parallel Systems

### 1. Use CPU-Intensive Tasks

Tasks should run for **at least 100 microseconds** each:

```rust
// ❌ BAD: Too light (~50ns)
for i in 0..1000 {
    sum += i;
}

// ✅ GOOD: Meaningful work (~1ms)
fn heavy_work(n: usize) -> u64 {
    let mut result = 0;
    for i in 0..n {
        result = result.wrapping_mul(1664525);
        result ^= result >> 13;
        // ... more operations
    }
    result
}
```

### 2. Always Use --release Mode

Debug builds are 10-100x slower:

```bash
# ❌ Debug mode
cargo run --example benchmark

# ✅ Release mode with optimizations
cargo run --release --example benchmark
```

### 3. Measure Speedup, Not Absolute Time

Focus on the **scaling curve**:

```
Workers | Time    | Speedup | Efficiency
--------|---------|---------|------------
1       | 1000ms  | 1.00x   | 100%
2       | 520ms   | 1.92x   | 96%
4       | 270ms   | 3.70x   | 93%
8       | 145ms   | 6.90x   | 86%
```

**Good scaling:** Efficiency > 80% up to your core count

### 4. Minimize Lock Contention

Original benchmark had a **hot lock**:

```rust
// ❌ BAD: Every task locks
*counter.lock().unwrap() += result;
```

Better approaches:

```rust
// ✅ GOOD: Thread-local accumulation
let result = do_heavy_work();
*counter.lock().unwrap() += result; // Still locks, but worth it

// ✅ BETTER: Return results, aggregate once
let results: Vec<u64> = taskflow.run_and_collect();
let sum: u64 = results.iter().sum();
```

### 5. Understanding Overhead Sources

**Work-stealing overhead:**
- Queue management: ~100-500ns per steal attempt
- Cache coherence: ~50-200ns per cache line transfer
- Thread wake-up: ~1-10µs if thread was sleeping

**When overhead matters:**
- Task duration < 10µs: Overhead dominates
- Task duration 10-100µs: Overhead significant
- Task duration > 100µs: Overhead negligible

## Running the Improved Benchmark

```bash
# Run the heavy benchmark (proper CPU-intensive tasks)
cargo run --release --example benchmark_heavy
```

Expected output:
```
1. CPU-Intensive Parallel Tasks
   (100 tasks, each with 1M operations)

  1 worker(s):  2.45s  (speedup: 1.00x, efficiency: 100.0%)
  2 worker(s):  1.28s  (speedup: 1.91x, efficiency: 95.7%)
  4 worker(s):  0.65s  (speedup: 3.77x, efficiency: 94.2%)
  8 worker(s):  0.35s  (speedup: 7.00x, efficiency: 87.5%)
```

## Key Takeaways

1. **Parallelism isn't free** - there's always overhead
2. **Make tasks substantial** - at least 100µs of work each
3. **Always use --release** for benchmarks
4. **Measure speedup and efficiency** - not just absolute time
5. **Minimize synchronization** - locks kill parallelism
6. **Your original benchmark was correct** - it just showed that tiny tasks don't benefit from parallelism!

## When to Use Parallel Task Graphs

✅ **Good use cases:**
- Image/video processing (each frame = task)
- Scientific simulations (each timestep or region)
- Build systems (each file compilation)
- Data pipelines (each batch of data)

❌ **Poor use cases:**
- Tiny mathematical operations
- Simple loops over small arrays
- Tasks that take < 10µs each
- Code with lots of locks/synchronization

## Further Reading

- [Amdahl's Law](https://en.wikipedia.org/wiki/Amdahl%27s_law)
- [Work-Stealing Scheduler Design](http://supertech.csail.mit.edu/papers/steal.pdf)
- [Performance Analysis of Parallel Programs](https://www.cs.cmu.edu/~scandal/cacm/node8.html)
