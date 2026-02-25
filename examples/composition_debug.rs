use taskflow_rs::{Executor, Taskflow, CompositionBuilder, TaskflowComposable};

fn main() {
    println!("=== Minimal Composition Debug ===\n");
    
    // Create a simple component
    let mut builder = CompositionBuilder::new();
    let task = builder.taskflow_mut().emplace(|| {
        println!("  [Component] Executing");
    }).name("component_task");
    
    println!("Component task ID: {}", task.id());
    
    builder.mark_entries(&[task.clone()]);
    builder.mark_exits(&[task]);
    
    let component = builder.build();
    
    println!("Component has {} entries, {} exits", 
             component.entries().len(), 
             component.exits().len());
    
    // Create main flow
    let mut main_flow = Taskflow::new();
    
    let start = main_flow.emplace(|| {
        println!("[Start]");
    }).name("start");
    
    println!("Start task ID: {}", start.id());
    
    let end = main_flow.emplace(|| {
        println!("[End]");
    }).name("end");
    
    println!("End task ID: {}", end.id());
    
    // Compose
    println!("\nComposing...");
    let instance = main_flow.compose(&component);
    
    println!("Instance has {} entries, {} exits",
             instance.entries().len(),
             instance.exits().len());
    
    if let Some(entry) = instance.entries().get(0) {
        println!("Entry ID: {}", entry.id());
        start.precede(entry);
    }
    
    if let Some(exit) = instance.exits().get(0) {
        println!("Exit ID: {}", exit.id());
        exit.precede(&end);
    }
    
    println!("\nGraph dump:");
    println!("{}", main_flow.dump());
    
    println!("\nExecuting...");
    let mut executor = Executor::new(2);
    executor.run(&main_flow).wait();
    
    println!("\n✓ Done");
}
