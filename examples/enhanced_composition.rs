use taskflow_rs::{
    Executor, Taskflow, CompositionBuilder, TaskflowComposable,
    ParameterizedComposition, CompositionParams,
};
use std::sync::{Arc, Mutex};

fn main() {
    println!("=== Enhanced Composition Demo ===\n");
    
    demo_cloneable_work();
    println!();
    
    demo_parameterized_composition();
    println!();
    
    demo_reusable_patterns();
    println!();
    
    demo_complex_pipeline();
}

/// Demo 1: Cloneable Work
fn demo_cloneable_work() {
    println!("1. Cloneable Work");
    println!("   Compositions with properly cloned work\n");
    
    let counter = Arc::new(Mutex::new(0));
    
    // Create a composition with cloneable work
    let mut builder = CompositionBuilder::new();
    
    let c = counter.clone();
    let task1 = builder.emplace_cloneable(move || {
        let mut count = c.lock().unwrap();
        *count += 1;
        println!("   Task 1: Count = {}", *count);
    });
    
    let c = counter.clone();
    let task2 = builder.emplace_cloneable(move || {
        let mut count = c.lock().unwrap();
        *count += 10;
        println!("   Task 2: Count = {}", *count);
    });
    
    task1.precede(&task2);
    
    builder.mark_entries(&[task1]);
    builder.mark_exits(&[task2]);
    
    let composition = builder.build();
    
    // Use the composition multiple times
    let mut executor = Executor::new(4);
    
    println!("   First execution:");
    let mut taskflow1 = Taskflow::new();
    taskflow1.compose(&composition);
    executor.run(&taskflow1).wait();
    
    println!("\n   Second execution:");
    let mut taskflow2 = Taskflow::new();
    taskflow2.compose(&composition);
    executor.run(&taskflow2).wait();
    
    println!("\n   Final counter: {}", *counter.lock().unwrap());
    println!("   ✓ Work was properly cloned and reused");
}

/// Demo 2: Parameterized Composition
fn demo_parameterized_composition() {
    println!("2. Parameterized Composition");
    println!("   Compositions that adapt based on parameters\n");
    
    // Create a parameterized composition
    let param_comp = ParameterizedComposition::new(|params| {
        let mut builder = CompositionBuilder::new();
        
        let num_tasks = params.get_int("num_tasks").unwrap_or(3);
        let message = params.get_string("message").unwrap_or("Task").to_string();
        
        let mut tasks = Vec::new();
        for i in 0..num_tasks {
            let msg = message.clone();
            let task = builder.emplace_cloneable(move || {
                println!("   {} {}", msg, i);
            });
            tasks.push(task);
        }
        
        // Chain tasks if there are any
        for i in 0..tasks.len().saturating_sub(1) {
            tasks[i].precede(&tasks[i + 1]);
        }
        
        if !tasks.is_empty() {
            builder.mark_entries(&[tasks[0].clone()]);
            builder.mark_exits(&[tasks.last().unwrap().clone()]);
        }
        
        builder.build()
    });
    
    let mut executor = Executor::new(4);
    
    // Use with different parameters
    println!("   Configuration 1: 3 tasks");
    let mut params1 = CompositionParams::new();
    params1.set_int("num_tasks", 3);
    params1.set_string("message", "Step".to_string());
    
    let mut taskflow1 = Taskflow::new();
    param_comp.compose_into(&mut taskflow1, &params1);
    executor.run(&taskflow1).wait();
    
    println!("\n   Configuration 2: 5 tasks");
    let mut params2 = CompositionParams::new();
    params2.set_int("num_tasks", 5);
    params2.set_string("message", "Phase".to_string());
    
    let mut taskflow2 = Taskflow::new();
    param_comp.compose_into(&mut taskflow2, &params2);
    executor.run(&taskflow2).wait();
    
    println!("\n   ✓ Parameterized composition works");
}

/// Demo 3: Reusable Patterns
fn demo_reusable_patterns() {
    println!("3. Reusable Patterns");
    println!("   Common task patterns as compositions\n");
    
    // Create a "map-reduce" pattern
    let map_reduce_pattern = ParameterizedComposition::new(|params| {
        let mut builder = CompositionBuilder::new();
        
        let num_mappers = params.get_int("num_mappers").unwrap_or(4) as usize;
        
        // Map phase
        let mut mappers = Vec::new();
        for i in 0..num_mappers {
            let mapper = builder.emplace_cloneable(move || {
                println!("   [Map {}] Processing data", i);
            });
            mappers.push(mapper);
        }
        
        // Reduce phase
        let reducer = builder.emplace_cloneable(|| {
            println!("   [Reduce] Aggregating results");
        });
        
        // Connect mappers to reducer
        for mapper in &mappers {
            mapper.precede(&reducer);
        }
        
        builder.mark_entries(&mappers);
        builder.mark_exits(&[reducer]);
        
        builder.build()
    });
    
    let mut executor = Executor::new(4);
    
    // Use the pattern
    println!("   Map-Reduce with 4 mappers:");
    let mut params = CompositionParams::new();
    params.set_int("num_mappers", 4);
    
    let mut taskflow = Taskflow::new();
    map_reduce_pattern.compose_into(&mut taskflow, &params);
    executor.run(&taskflow).wait();
    
    println!("\n   ✓ Reusable pattern works");
}

/// Demo 4: Complex Pipeline
fn demo_complex_pipeline() {
    println!("4. Complex Pipeline");
    println!("   Multiple compositions chained together\n");
    
    // Create a preprocessing composition
    let preprocess = ParameterizedComposition::new(|params| {
        let mut builder = CompositionBuilder::new();
        
        let validate = builder.emplace_cloneable(|| {
            println!("   [Preprocess] Validating input");
        });
        
        let clean = builder.emplace_cloneable(|| {
            println!("   [Preprocess] Cleaning data");
        });
        
        validate.precede(&clean);
        
        builder.mark_entries(&[validate]);
        builder.mark_exits(&[clean]);
        
        builder.build()
    });
    
    // Create a processing composition
    let process = ParameterizedComposition::new(|params| {
        let mut builder = CompositionBuilder::new();
        
        let num_workers = params.get_int("workers").unwrap_or(2) as usize;
        
        let mut workers = Vec::new();
        for i in 0..num_workers {
            let worker = builder.emplace_cloneable(move || {
                println!("   [Process] Worker {} processing", i);
            });
            workers.push(worker);
        }
        
        builder.mark_entries(&workers);
        builder.mark_exits(&workers);
        
        builder.build()
    });
    
    // Create a postprocessing composition
    let postprocess = ParameterizedComposition::new(|_params| {
        let mut builder = CompositionBuilder::new();
        
        let aggregate = builder.emplace_cloneable(|| {
            println!("   [Postprocess] Aggregating results");
        });
        
        let save = builder.emplace_cloneable(|| {
            println!("   [Postprocess] Saving output");
        });
        
        aggregate.precede(&save);
        
        builder.mark_entries(&[aggregate]);
        builder.mark_exits(&[save]);
        
        builder.build()
    });
    
    // Build the complete pipeline
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();
    
    let params = CompositionParams::new();
    let mut process_params = CompositionParams::new();
    process_params.set_int("workers", 3);
    
    // Compose all stages
    let pre_instance = preprocess.compose_into(&mut taskflow, &params);
    let proc_instance = process.compose_into(&mut taskflow, &process_params);
    let post_instance = postprocess.compose_into(&mut taskflow, &params);
    
    // Connect stages
    for pre_exit in pre_instance.exits() {
        for proc_entry in proc_instance.entries() {
            pre_exit.precede(proc_entry);
        }
    }
    
    for proc_exit in proc_instance.exits() {
        for post_entry in post_instance.entries() {
            proc_exit.precede(post_entry);
        }
    }
    
    println!("   Executing complex pipeline:");
    executor.run(&taskflow).wait();
    
    println!("\n   ✓ Complex pipeline with multiple compositions works");
}
