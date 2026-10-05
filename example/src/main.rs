//! Runnable library-host example. The recording backend never touches the desktop.
use scriptaro_core::yaml;
use scriptaro_engine::{Engine, PlaybackEvent, RunOptions};
use scriptaro_platform::recording::{Operation, RecordingBackend};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = yaml::from_str(include_str!("../sequence.yaml"))?;
    let mut backend = RecordingBackend::default();
    let engine = Engine::new(
        &mut backend,
        RunOptions {
            skip_delays: true,
            ..Default::default()
        },
    );
    let mut events = engine.subscribe();
    let controller = engine.controller();
    println!("Scriptaro example — simulated desktop, no input is sent");
    println!("Controller starts in {:?}", controller.state());
    let report = engine.run(&script).await?;
    while let Ok(event) = events.try_recv() {
        if let PlaybackEvent::StepStarted { step, action } = event {
            println!("  {step}: {action}");
        }
    }
    let typed: String = backend
        .operations
        .iter()
        .filter_map(|operation| match operation {
            Operation::Character(character) => Some(*character),
            _ => None,
        })
        .collect();
    println!("\nSimulated document:\n{typed}");
    println!("{:?}: {} steps", report.status, report.completed_steps);
    Ok(())
}
