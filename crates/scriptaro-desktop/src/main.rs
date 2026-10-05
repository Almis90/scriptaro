#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(tokio::task::LocalSet::new().run_until(macos::run()))
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "Scriptaro's native desktop interface currently supports macOS. The CLI supports validation and simulation on this platform."
    );
    std::process::exit(1);
}
