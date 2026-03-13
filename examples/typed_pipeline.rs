use taskflow_rs::{SimplePipeline, TypeSafePipeline};

fn main() {
    println!("=== Type-Safe Pipeline Demo ===\n");

    demo_type_safe_pipeline();
    println!();

    demo_simple_pipeline();
    println!();

    demo_data_processing();
    println!();

    demo_compile_time_safety();
}

/// Demo 1: Type-Safe Pipeline
fn demo_type_safe_pipeline() {
    println!("1. Type-Safe Pipeline");
    println!("   Compile-time type checking for pipeline stages\n");

    // Build a pipeline with type checking
    let pipeline = TypeSafePipeline::new()
        .stage(|x: i32| {
            println!("   Stage 1: Multiply {} by 2", x);
            x * 2
        })
        .stage(|x: i32| {
            println!("   Stage 2: Add 10 to {}", x);
            x + 10
        })
        .stage(|x: i32| {
            println!("   Stage 3: Convert {} to f64", x);
            x as f64
        })
        .stage(|x: f64| {
            println!("   Stage 4: Format {:.2} as string", x);
            format!("{:.2}", x)
        });

    println!("\n   Executing pipeline with input: 5");
    let result = pipeline.execute(5);

    println!("\n   Final result: {}", result);
    println!("   Expected: 20.00");
    assert_eq!(result, "20.00");
    println!("   ✓ Type-safe pipeline executed successfully");
}

/// Demo 2: Simple Pipeline
fn demo_simple_pipeline() {
    println!("2. Simple Pipeline");
    println!("   In-place mutation pipeline\n");

    #[derive(Debug)]
    struct Data {
        value: i32,
        multiplier: i32,
    }

    let pipeline = SimplePipeline::new()
        .stage(|data: &mut Data| {
            println!(
                "   Stage 1: Multiply value {} by {}",
                data.value, data.multiplier
            );
            data.value *= data.multiplier;
        })
        .stage(|data: &mut Data| {
            println!("   Stage 2: Add 100 to value {}", data.value);
            data.value += 100;
        })
        .stage(|data: &mut Data| {
            println!("   Stage 3: Square value {}", data.value);
            data.value = data.value * data.value;
        });

    let input = Data {
        value: 5,
        multiplier: 3,
    };
    println!("\n   Executing pipeline with: {:?}", input);

    let result = pipeline.execute(input);

    println!("\n   Final result: {:?}", result);
    println!("   Expected: value = 13225 (((5*3)+100)^2)");
    assert_eq!(result.value, 13225);
    println!("   ✓ Simple pipeline executed successfully");
}

/// Demo 3: Data Processing Pipeline
fn demo_data_processing() {
    println!("3. Data Processing Pipeline");
    println!("   Real-world data transformation\n");

    // Parse CSV -> Filter -> Aggregate -> Format
    let pipeline = TypeSafePipeline::new()
        .stage(|csv: &str| {
            println!("   Stage 1: Parse CSV");
            // Simulate parsing
            csv.lines()
                .skip(1) // Skip header
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        parts[1].parse::<i32>().ok()
                    } else {
                        None
                    }
                })
                .collect::<Vec<i32>>()
        })
        .stage(|numbers: Vec<i32>| {
            println!("   Stage 2: Filter (keep > 50)");
            numbers
                .into_iter()
                .filter(|&x| x > 50)
                .collect::<Vec<i32>>()
        })
        .stage(|numbers: Vec<i32>| {
            println!("   Stage 3: Calculate sum");
            numbers.iter().sum::<i32>()
        })
        .stage(|sum: i32| {
            println!("   Stage 4: Format result");
            format!("Total: ${}", sum)
        });

    let csv_data = "name,amount\nAlice,100\nBob,30\nCarol,75\nDave,45\nEve,90";
    println!("\n   Input CSV:\n{}\n", csv_data);

    let result = pipeline.execute(csv_data);

    println!("\n   Final result: {}", result);
    println!("   Expected: Total: $265 (100 + 75 + 90)");
    assert_eq!(result, "Total: $265");
    println!("   ✓ Data processing pipeline works");
}

/// Demo 4: Compile-Time Safety
fn demo_compile_time_safety() {
    println!("4. Compile-Time Safety");
    println!("   Type mismatches caught at compile time\n");

    // This compiles - types match
    let _good_pipeline = TypeSafePipeline::new()
        .stage(|x: i32| x * 2) // i32 -> i32 ✓
        .stage(|x: i32| x as f64) // i32 -> f64 ✓
        .stage(|x: f64| format!("{}", x)); // f64 -> String ✓

    println!("   ✓ Valid pipeline compiles");

    // This would NOT compile - type mismatch:
    // let _bad_pipeline = TypeSafePipeline::new()
    //     .stage(|x: i32| x * 2)           // i32 -> i32
    //     .stage(|x: f64| x + 1.0);        // ERROR: expects i32, got f64

    println!("   ✓ Invalid pipeline rejected by compiler");

    // Demonstrate type inference
    let pipeline = TypeSafePipeline::new()
        .stage(|x: i32| x * 2)
        .stage(|x| x + 10) // Type inferred as i32
        .stage(|x| x as f64); // Type inferred as i32 -> f64

    let result = pipeline.execute(5);
    println!("\n   Type inference result: {}", result);
    assert_eq!(result, 20.0);
    println!("   ✓ Type inference works correctly");
}
