use taskflow_rs::{Executor, Taskflow, Composition, CompositionBuilder, TaskflowComposable};
use std::sync::{Arc, Mutex};

fn main() {
    println!("=== Task Graph Composition Demo ===\n");
    
    demo_simple_composition();
    println!();
    
    demo_reusable_module();
    println!();
    
    demo_sequential_composition();
    println!();
    
    demo_parallel_composition();
}

/// Demo 1: Simple composition - combine two taskflows
fn demo_simple_composition() {
    println!("1. Simple Composition");
    println!("   Combining two task graphs\n");
    
    let mut executor = Executor::new(4);
    
    // Create first component: data preprocessing
    let preprocessing = create_preprocessing_component();
    
    // Create second component: data analysis
    let analysis = create_analysis_component();
    
    // Compose them into a main taskflow
    let mut main_flow = Taskflow::new();
    
    println!("   Composing preprocessing...");
    let prep_instance = main_flow.compose(&preprocessing);
    
    println!("   Composing analysis...");
    let analysis_instance = main_flow.compose(&analysis);
    
    // Connect preprocessing outputs to analysis inputs
    for exit in prep_instance.exits() {
        for entry in analysis_instance.entries() {
            exit.precede(entry);
        }
    }
    
    println!("   Executing composed workflow...\n");
    executor.run(&main_flow).wait();
    
    println!("   ✓ Composition complete");
}

/// Demo 2: Reusable module - use the same component multiple times
fn demo_reusable_module() {
    println!("2. Reusable Module");
    println!("   Using the same component multiple times\n");
    println!("   Note: Composed tasks show '[Composed: name]' due to work cloning limitation\n");
    
    let mut executor = Executor::new(4);
    
    // Create a reusable data processing module
    let processor = create_processor_module();
    
    // Use it three times in parallel
    let mut main_flow = Taskflow::new();
    
    let start = main_flow.emplace(|| {
        println!("   [Start] Beginning parallel processing");
    }).name("start");
    
    let end = main_flow.emplace(|| {
        println!("   [End] All processing complete");
    }).name("end");
    
    println!("   Creating three instances of the processor...\n");
    
    for i in 0..3 {
        println!("   Instance {}:", i);
        let instance = main_flow.compose(&processor);
        
        println!("     Entries: {}, Exits: {}", instance.entries().len(), instance.exits().len());
        
        // Connect start to this instance's entries
        for entry in instance.entries() {
            start.precede(entry);
        }
        
        // Connect this instance's exits to end
        for exit in instance.exits() {
            exit.precede(&end);
        }
    }
    
    println!("\n   Executing parallel instances...\n");
    executor.run(&main_flow).wait();
    
    println!("   ✓ Reusable module demo complete");
}

/// Demo 3: Sequential composition - chain components
fn demo_sequential_composition() {
    println!("3. Sequential Composition");
    println!("   Chaining multiple components\n");
    
    let mut executor = Executor::new(4);
    let mut main_flow = Taskflow::new();
    
    // Create pipeline stages as composable components
    let stage1 = create_stage_component("Stage 1", "Extract");
    let stage2 = create_stage_component("Stage 2", "Transform");
    let stage3 = create_stage_component("Stage 3", "Load");
    
    println!("   Building ETL pipeline...\n");
    
    // Compose them sequentially
    let inst1 = main_flow.compose(&stage1);
    let inst2 = main_flow.compose(&stage2);
    let inst3 = main_flow.compose(&stage3);
    
    // Chain them together
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
    
    println!("   Executing ETL pipeline...\n");
    executor.run(&main_flow).wait();
    
    println!("   ✓ Sequential composition complete");
}

/// Demo 4: Parallel composition - fan-out/fan-in pattern
fn demo_parallel_composition() {
    println!("4. Parallel Composition");
    println!("   Fan-out and fan-in pattern\n");
    
    let mut executor = Executor::new(4);
    let mut main_flow = Taskflow::new();
    
    let split = main_flow.emplace(|| {
        println!("   [Split] Distributing work");
    }).name("split");
    
    let merge = main_flow.emplace(|| {
        println!("   [Merge] Combining results");
    }).name("merge");
    
    // Create three parallel processing branches
    let branch_a = create_stage_component("Branch A", "Process A");
    let branch_b = create_stage_component("Branch B", "Process B");
    let branch_c = create_stage_component("Branch C", "Process C");
    
    println!("   Creating parallel branches...\n");
    
    // Compose all branches
    let inst_a = main_flow.compose(&branch_a);
    let inst_b = main_flow.compose(&branch_b);
    let inst_c = main_flow.compose(&branch_c);
    
    // Connect split to all branches (fan-out)
    for inst in [&inst_a, &inst_b, &inst_c] {
        for entry in inst.entries() {
            split.precede(entry);
        }
    }
    
    // Connect all branches to merge (fan-in)
    for inst in [&inst_a, &inst_b, &inst_c] {
        for exit in inst.exits() {
            exit.precede(&merge);
        }
    }
    
    println!("   Executing parallel branches...\n");
    executor.run(&main_flow).wait();
    
    println!("   ✓ Parallel composition complete");
}

// Helper functions to create reusable components

fn create_preprocessing_component() -> Composition {
    let mut builder = CompositionBuilder::new();
    
    let load = builder.taskflow_mut().emplace(|| {
        println!("   [Preprocessing] Loading data");
    }).name("load");
    
    let clean = builder.taskflow_mut().emplace(|| {
        println!("   [Preprocessing] Cleaning data");
    }).name("clean");
    
    let validate = builder.taskflow_mut().emplace(|| {
        println!("   [Preprocessing] Validating data");
    }).name("validate");
    
    load.precede(&clean);
    clean.precede(&validate);
    
    builder.mark_entries(&[load]);
    builder.mark_exits(&[validate]);
    
    builder.build()
}

fn create_analysis_component() -> Composition {
    let mut builder = CompositionBuilder::new();
    
    let analyze = builder.taskflow_mut().emplace(|| {
        println!("   [Analysis] Analyzing data");
    }).name("analyze");
    
    let report = builder.taskflow_mut().emplace(|| {
        println!("   [Analysis] Generating report");
    }).name("report");
    
    analyze.precede(&report);
    
    builder.mark_entries(&[analyze]);
    builder.mark_exits(&[report]);
    
    builder.build()
}

fn create_processor_module() -> Composition {
    let mut builder = CompositionBuilder::new();
    
    let process = builder.taskflow_mut().emplace(|| {
        println!("     [Module] Processing data");
    }).name("process");
    
    builder.mark_entries(&[process.clone()]);
    builder.mark_exits(&[process]);
    
    builder.build()
}

fn create_stage_component(stage_name: &str, action: &str) -> Composition {
    let name = stage_name.to_string();
    let action = action.to_string();
    
    let mut builder = CompositionBuilder::new();
    
    let task = builder.taskflow_mut().emplace(move || {
        println!("   [{}] {}", name, action);
    }).name(stage_name);
    
    builder.mark_entries(&[task.clone()]);
    builder.mark_exits(&[task]);
    
    builder.build()
}
