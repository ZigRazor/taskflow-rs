use taskflow_rs::{Executor, Taskflow};

fn main() {
    println!("=== Example 1: Basic Task Graph ===");
    basic_task_graph();

    println!("\n=== Example 2: Diamond Pattern ===");
    diamond_pattern();

    println!("\n=== Example 3: Subflow ===");
    subflow_example();

    println!("\n=== Example 4: Graph Visualization ===");
    graph_visualization();
}

fn basic_task_graph() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    let a = taskflow
        .emplace(|| {
            println!("Task A executing");
        })
        .name("A");

    let b = taskflow
        .emplace(|| {
            println!("Task B executing");
        })
        .name("B");

    let c = taskflow
        .emplace(|| {
            println!("Task C executing");
        })
        .name("C");

    let d = taskflow
        .emplace(|| {
            println!("Task D executing");
        })
        .name("D");

    // A runs before B and C
    a.precede(&b);
    a.precede(&c);

    // D runs after B and C
    d.succeed(&b);
    d.succeed(&c);

    executor.run(&taskflow).wait();
}

fn diamond_pattern() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    let start = taskflow
        .emplace(|| {
            println!("Start task");
        })
        .name("start");

    let left = taskflow
        .emplace(|| {
            println!("Left branch");
            std::thread::sleep(std::time::Duration::from_millis(100));
        })
        .name("left");

    let right = taskflow
        .emplace(|| {
            println!("Right branch");
            std::thread::sleep(std::time::Duration::from_millis(100));
        })
        .name("right");

    let end = taskflow
        .emplace(|| {
            println!("End task");
        })
        .name("end");

    start.precede(&left);
    start.precede(&right);
    end.succeed(&left);
    end.succeed(&right);

    executor.run(&taskflow).wait();
}

fn subflow_example() {
    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    let a = taskflow
        .emplace(|| {
            println!("Task A");
        })
        .name("A");

    let b = taskflow
        .emplace_subflow(|subflow| {
            println!("Task B - creating subflow");

            let b1 = subflow
                .emplace(|| {
                    println!("  Subflow task B1");
                })
                .name("B1");

            let b2 = subflow
                .emplace(|| {
                    println!("  Subflow task B2");
                })
                .name("B2");

            let b3 = subflow
                .emplace(|| {
                    println!("  Subflow task B3");
                })
                .name("B3");

            b1.precede(&b3);
            b2.precede(&b3);
        })
        .name("B");

    let c = taskflow
        .emplace(|| {
            println!("Task C");
        })
        .name("C");

    a.precede(&b);
    c.succeed(&b);

    executor.run(&taskflow).wait();
}

fn graph_visualization() {
    let mut taskflow = Taskflow::new();

    let a = taskflow.emplace(|| {}).name("A");
    let b = taskflow.emplace(|| {}).name("B");
    let c = taskflow.emplace(|| {}).name("C");
    let d = taskflow.emplace(|| {}).name("D");
    let e = taskflow.emplace(|| {}).name("E");

    a.precede(&b);
    a.precede(&c);
    b.precede(&d);
    c.precede(&d);
    d.precede(&e);

    println!("{}", taskflow.dump());
    println!("You can visualize this at https://dreampuf.github.io/GraphvizOnline/");
}
