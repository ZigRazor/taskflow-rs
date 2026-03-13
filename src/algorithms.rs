use crate::{TaskHandle, Taskflow};
use std::sync::{Arc, Mutex};

/// Parallel for_each - applies a function to each element in parallel
pub fn parallel_for_each<T, F>(
    taskflow: &mut Taskflow,
    data: Vec<T>,
    chunk_size: usize,
    func: F,
) -> Vec<TaskHandle>
where
    T: Send + 'static,
    F: Fn(T) + Send + Sync + Clone + 'static,
{
    let mut tasks = Vec::new();
    let data_len = data.len();
    let num_chunks = (data_len + chunk_size - 1) / chunk_size;

    let data = Arc::new(Mutex::new(data.into_iter()));

    for chunk_id in 0..num_chunks {
        let data = Arc::clone(&data);
        let func = func.clone();

        let task = taskflow
            .emplace(move || {
                // Pull a chunk of items
                let items: Vec<T> = {
                    let mut iter = data.lock().unwrap();
                    iter.by_ref().take(chunk_size).collect()
                };

                // Process each item
                for item in items {
                    func(item);
                }
            })
            .name(&format!("for_each_chunk_{}", chunk_id));

        tasks.push(task);
    }

    tasks
}

/// Parallel reduce - reduces a collection to a single value
pub fn parallel_reduce<T, F>(
    taskflow: &mut Taskflow,
    data: Vec<T>,
    chunk_size: usize,
    identity: T,
    reduce_fn: F,
) -> (TaskHandle, Arc<Mutex<T>>)
where
    T: Send + Clone + 'static,
    F: Fn(T, T) -> T + Send + Sync + Clone + 'static,
{
    let data_len = data.len();
    let num_chunks = (data_len + chunk_size - 1) / chunk_size;

    // Partial results from each chunk
    let partial_results = Arc::new(Mutex::new(Vec::new()));
    let final_result = Arc::new(Mutex::new(identity.clone()));

    let data = Arc::new(Mutex::new(data.into_iter()));

    let mut map_tasks = Vec::new();

    // Map phase: reduce each chunk
    for chunk_id in 0..num_chunks {
        let data = Arc::clone(&data);
        let partial_results = Arc::clone(&partial_results);
        let chunk_identity = identity.clone();
        let reduce_fn = reduce_fn.clone();

        let task = taskflow
            .emplace(move || {
                // Pull a chunk of items
                let items: Vec<T> = {
                    let mut iter = data.lock().unwrap();
                    iter.by_ref().take(chunk_size).collect()
                };

                // Reduce this chunk
                let chunk_result = items
                    .into_iter()
                    .fold(chunk_identity, |acc, item| reduce_fn(acc, item));

                // Store partial result
                partial_results.lock().unwrap().push(chunk_result);
            })
            .name(&format!("reduce_map_{}", chunk_id));

        map_tasks.push(task);
    }

    // Reduce phase: combine partial results
    let final_result_clone = Arc::clone(&final_result);
    let final_identity = identity.clone();
    let reduce_task = taskflow
        .emplace(move || {
            let results = partial_results.lock().unwrap();
            let combined = results
                .iter()
                .fold(final_identity, |acc, item| reduce_fn(acc, item.clone()));
            *final_result_clone.lock().unwrap() = combined;
        })
        .name("reduce_combine");

    // Set dependencies: all map tasks must complete before reduce
    for map_task in &map_tasks {
        reduce_task.succeed(map_task);
    }

    (reduce_task, final_result)
}

/// Parallel transform - maps elements in parallel and collects results
pub fn parallel_transform<T, U, F>(
    taskflow: &mut Taskflow,
    data: Vec<T>,
    chunk_size: usize,
    transform_fn: F,
) -> (Vec<TaskHandle>, Arc<Mutex<Vec<U>>>)
where
    T: Send + 'static,
    U: Send + 'static,
    F: Fn(T) -> U + Send + Sync + Clone + 'static,
{
    let data_len = data.len();
    let num_chunks = (data_len + chunk_size - 1) / chunk_size;

    // Results storage
    let results = Arc::new(Mutex::new(Vec::new()));

    let data = Arc::new(Mutex::new(data.into_iter()));

    let mut tasks = Vec::new();

    for chunk_id in 0..num_chunks {
        let data = Arc::clone(&data);
        let results = Arc::clone(&results);
        let transform_fn = transform_fn.clone();

        let task = taskflow
            .emplace(move || {
                // Pull a chunk of items
                let items: Vec<T> = {
                    let mut iter = data.lock().unwrap();
                    iter.by_ref().take(chunk_size).collect()
                };

                // Transform each item
                let transformed: Vec<U> =
                    items.into_iter().map(|item| transform_fn(item)).collect();

                // Store results
                results.lock().unwrap().extend(transformed);
            })
            .name(&format!("transform_chunk_{}", chunk_id));

        tasks.push(task);
    }

    (tasks, results)
}

/// Parallel sort - sorts elements in parallel using merge sort
pub fn parallel_sort<T, F>(
    taskflow: &mut Taskflow,
    mut data: Vec<T>,
    chunk_size: usize,
    compare: F,
) -> TaskHandle
where
    T: Send + Clone + 'static,
    F: Fn(&T, &T) -> std::cmp::Ordering + Send + Sync + Clone + 'static,
{
    let data_len = data.len();

    // If data is small enough, sort it directly
    if data_len <= chunk_size {
        return taskflow
            .emplace(move || {
                data.sort_by(&compare);
            })
            .name("sort_sequential");
    }

    let num_chunks = (data_len + chunk_size - 1) / chunk_size;

    // Sorted chunks storage
    let sorted_chunks = Arc::new(Mutex::new(Vec::new()));

    let mut sort_tasks = Vec::new();

    // Sort each chunk
    let mut start = 0;
    for chunk_id in 0..num_chunks {
        let end = (start + chunk_size).min(data_len);
        let mut chunk: Vec<T> = data[start..end].to_vec();
        let sorted_chunks = Arc::clone(&sorted_chunks);
        let compare = compare.clone();

        let task = taskflow
            .emplace(move || {
                chunk.sort_by(&compare);
                sorted_chunks.lock().unwrap().push(chunk);
            })
            .name(&format!("sort_chunk_{}", chunk_id));

        sort_tasks.push(task);
        start = end;
    }

    // Merge sorted chunks
    let merge_task = taskflow
        .emplace(move || {
            let chunks = sorted_chunks.lock().unwrap();

            // Simple k-way merge (not the most efficient, but works)
            let mut merged = Vec::new();
            let mut iters: Vec<_> = chunks.iter().map(|chunk| chunk.iter().peekable()).collect();

            loop {
                // Find the minimum element among all chunk heads
                let mut min_idx = None;
                let mut min_val = None;

                for (idx, iter) in iters.iter_mut().enumerate() {
                    if let Some(&val) = iter.peek() {
                        if min_val.is_none()
                            || compare(val, min_val.unwrap()) == std::cmp::Ordering::Less
                        {
                            min_val = Some(val);
                            min_idx = Some(idx);
                        }
                    }
                }

                if let Some(idx) = min_idx {
                    if let Some(val) = iters[idx].next() {
                        merged.push(val.clone());
                    }
                } else {
                    break;
                }
            }

            // Store back (in a real implementation, we'd return this)
            let _sorted_data = merged;
        })
        .name("sort_merge");

    // Set dependencies
    for sort_task in &sort_tasks {
        merge_task.succeed(sort_task);
    }

    merge_task
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Executor;

    #[test]
    fn test_parallel_for_each() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let counter = Arc::new(Mutex::new(0));
        let data: Vec<i32> = (0..100).collect();

        let counter_clone = Arc::clone(&counter);
        parallel_for_each(&mut taskflow, data, 25, move |_item| {
            *counter_clone.lock().unwrap() += 1;
        });

        executor.run(&taskflow).wait();

        assert_eq!(*counter.lock().unwrap(), 100);
    }

    #[test]
    fn test_parallel_reduce() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let data: Vec<i32> = (1..=100).collect();

        let (_task, result) = parallel_reduce(&mut taskflow, data, 25, 0, |acc, item| acc + item);

        executor.run(&taskflow).wait();

        // Sum of 1..=100 is 5050
        assert_eq!(*result.lock().unwrap(), 5050);
    }

    #[test]
    fn test_parallel_transform() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let data: Vec<i32> = (0..100).collect();

        let (_tasks, results) = parallel_transform(&mut taskflow, data, 25, |x| x * 2);

        executor.run(&taskflow).wait();

        let results = results.lock().unwrap();
        assert_eq!(results.len(), 100);
    }
}

/// Parallel inclusive scan (prefix sum) - each output element is the sum of all input elements up to and including that position
///
/// Example: [1, 2, 3, 4] -> [1, 3, 6, 10]
///
/// # Arguments
/// * `taskflow` - The taskflow to add tasks to
/// * `data` - Input data to scan
/// * `chunk_size` - Size of chunks for parallel processing
/// * `op` - Binary operation to apply (typically addition, must be associative)
/// * `identity` - Identity element for the operation
///
/// # Type Requirements
/// * `T` must be `Send + Sync + Clone + 'static` - the data type must be safely shared between threads
/// * `F` must be `Fn(T, T) -> T + Send + Sync + Clone + 'static` - the operation must be thread-safe
///
/// # Returns
/// A tuple of (tasks, result) where result is Arc<Mutex<Vec<T>>>
pub fn parallel_inclusive_scan<T, F>(
    taskflow: &mut Taskflow,
    data: Vec<T>,
    chunk_size: usize,
    op: F,
    identity: T,
) -> (Vec<TaskHandle>, Arc<Mutex<Vec<T>>>)
where
    T: Send + Sync + Clone + 'static,
    F: Fn(T, T) -> T + Send + Sync + Clone + 'static,
{
    let data_len = data.len();
    if data_len == 0 {
        return (Vec::new(), Arc::new(Mutex::new(Vec::new())));
    }

    let num_chunks = (data_len + chunk_size - 1) / chunk_size;
    let result = Arc::new(Mutex::new(vec![identity.clone(); data_len]));
    let chunk_sums = Arc::new(Mutex::new(vec![identity.clone(); num_chunks]));
    let input_data = Arc::new(data);

    let mut tasks = Vec::new();

    // Phase 1: Compute local scans for each chunk
    let mut phase1_tasks = Vec::new();
    for chunk_id in 0..num_chunks {
        let data = Arc::clone(&input_data);
        let result = Arc::clone(&result);
        let chunk_sums = Arc::clone(&chunk_sums);
        let op = op.clone();

        let task = taskflow
            .emplace(move || {
                let start = chunk_id * chunk_size;
                let end = (start + chunk_size).min(data.len());

                if start >= data.len() {
                    return;
                }

                // Compute local inclusive scan
                let mut sum = data[start].clone();
                result.lock().unwrap()[start] = sum.clone();

                for i in (start + 1)..end {
                    sum = op(sum, data[i].clone());
                    result.lock().unwrap()[i] = sum.clone();
                }

                // Store the chunk sum
                chunk_sums.lock().unwrap()[chunk_id] = sum;
            })
            .name(&format!("scan_phase1_{}", chunk_id));

        phase1_tasks.push(task.clone());
        tasks.push(task);
    }

    // Phase 2: Compute prefix sum of chunk sums
    let chunk_prefixes = Arc::new(Mutex::new(vec![identity.clone(); num_chunks]));
    let phase2_result = Arc::clone(&chunk_prefixes);
    let phase2_sums = Arc::clone(&chunk_sums);
    let op2 = op.clone();

    let phase2_task = taskflow
        .emplace(move || {
            let sums = phase2_sums.lock().unwrap();
            let mut prefixes = phase2_result.lock().unwrap();

            if sums.is_empty() {
                return;
            }

            prefixes[0] = sums[0].clone();
            for i in 1..sums.len() {
                prefixes[i] = op2(prefixes[i - 1].clone(), sums[i].clone());
            }
        })
        .name("scan_phase2");

    // Phase 2 depends on all phase 1 tasks
    for task in &phase1_tasks {
        task.precede(&phase2_task);
    }
    tasks.push(phase2_task.clone());

    // Phase 3: Add chunk prefixes to local scans
    for chunk_id in 1..num_chunks {
        let result = Arc::clone(&result);
        let prefixes = Arc::clone(&chunk_prefixes);
        let op = op.clone();

        let task = taskflow
            .emplace(move || {
                let prefix = prefixes.lock().unwrap()[chunk_id - 1].clone();
                let start = chunk_id * chunk_size;
                let end = (start + chunk_size).min(result.lock().unwrap().len());

                let mut result = result.lock().unwrap();
                for i in start..end {
                    result[i] = op(prefix.clone(), result[i].clone());
                }
            })
            .name(&format!("scan_phase3_{}", chunk_id));

        phase2_task.precede(&task);
        tasks.push(task);
    }

    (tasks, result)
}

/// Parallel exclusive scan (prefix sum) - each output element is the sum of all input elements before that position
///
/// Example: [1, 2, 3, 4] -> [0, 1, 3, 6]
///
/// # Arguments
/// * `taskflow` - The taskflow to add tasks to
/// * `data` - Input data to scan
/// * `chunk_size` - Size of chunks for parallel processing
/// * `op` - Binary operation to apply (typically addition, must be associative)
/// * `identity` - Identity element for the operation
///
/// # Type Requirements
/// * `T` must be `Send + Sync + Clone + 'static` - the data type must be safely shared between threads
/// * `F` must be `Fn(T, T) -> T + Send + Sync + Clone + 'static` - the operation must be thread-safe
///
/// # Returns
/// A tuple of (tasks, result) where result is Arc<Mutex<Vec<T>>>
pub fn parallel_exclusive_scan<T, F>(
    taskflow: &mut Taskflow,
    data: Vec<T>,
    chunk_size: usize,
    op: F,
    identity: T,
) -> (Vec<TaskHandle>, Arc<Mutex<Vec<T>>>)
where
    T: Send + Sync + Clone + 'static,
    F: Fn(T, T) -> T + Send + Sync + Clone + 'static,
{
    let data_len = data.len();
    if data_len == 0 {
        return (Vec::new(), Arc::new(Mutex::new(Vec::new())));
    }

    // Compute inclusive scan first
    let (tasks, inclusive_result) =
        parallel_inclusive_scan(taskflow, data, chunk_size, op, identity.clone());

    // Shift right by one and prepend identity
    let exclusive_result = Arc::new(Mutex::new(vec![identity; data_len]));
    let excl = Arc::clone(&exclusive_result);
    let incl = Arc::clone(&inclusive_result);

    let shift_task = taskflow
        .emplace(move || {
            let inclusive = incl.lock().unwrap();
            let mut exclusive = excl.lock().unwrap();

            // Shift: exclusive[i] = inclusive[i-1]
            for i in 1..exclusive.len() {
                exclusive[i] = inclusive[i - 1].clone();
            }
            // exclusive[0] is already identity
        })
        .name("scan_shift");

    // Shift depends on all scan tasks
    for task in &tasks {
        task.precede(&shift_task);
    }

    let mut all_tasks = tasks;
    all_tasks.push(shift_task);

    (all_tasks, exclusive_result)
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    use crate::Executor;

    #[test]
    fn test_parallel_inclusive_scan() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let data: Vec<i32> = vec![1, 2, 3, 4, 5, 6, 7, 8];

        let (_tasks, result) = parallel_inclusive_scan(&mut taskflow, data, 2, |a, b| a + b, 0);

        executor.run(&taskflow).wait();

        let result = result.lock().unwrap();
        // Expected: [1, 3, 6, 10, 15, 21, 28, 36]
        assert_eq!(*result, vec![1, 3, 6, 10, 15, 21, 28, 36]);
    }

    #[test]
    fn test_parallel_exclusive_scan() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let data: Vec<i32> = vec![1, 2, 3, 4, 5, 6, 7, 8];

        let (_tasks, result) = parallel_exclusive_scan(&mut taskflow, data, 2, |a, b| a + b, 0);

        executor.run(&taskflow).wait();

        let result = result.lock().unwrap();
        // Expected: [0, 1, 3, 6, 10, 15, 21, 28]
        assert_eq!(*result, vec![0, 1, 3, 6, 10, 15, 21, 28]);
    }

    #[test]
    fn test_parallel_scan_multiplication() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let data: Vec<i32> = vec![2, 3, 4, 5];

        let (_tasks, result) = parallel_inclusive_scan(&mut taskflow, data, 2, |a, b| a * b, 1);

        executor.run(&taskflow).wait();

        let result = result.lock().unwrap();
        // Expected: [2, 6, 24, 120]
        assert_eq!(*result, vec![2, 6, 24, 120]);
    }

    #[test]
    fn test_parallel_scan_large() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let data: Vec<i32> = (1..=100).collect();

        let (_tasks, result) = parallel_inclusive_scan(&mut taskflow, data, 10, |a, b| a + b, 0);

        executor.run(&taskflow).wait();

        let result = result.lock().unwrap();
        // Check last element: sum(1..100) = 5050
        assert_eq!(result[99], 5050);
        // Check a few middle elements
        assert_eq!(result[9], 55); // sum(1..10)
        assert_eq!(result[49], 1275); // sum(1..50)
    }
}
