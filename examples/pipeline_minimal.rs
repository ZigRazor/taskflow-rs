// Minimal pipeline example to test exports
fn main() {
    use taskflow_rs::pipeline::{ConcurrentPipeline, StageType, Token};

    println!("Testing pipeline exports...");

    let pipeline = ConcurrentPipeline::new(10, 100);
    pipeline.push(42).unwrap();

    if let Some(token) = pipeline.try_pop() {
        println!("Got token #{}: {}", token.index, token.data);
    }

    let stage = StageType::Serial;
    println!("Stage type: {:?}", stage);

    println!("✓ Pipeline exports work!");
}
