//! Native macOS implementation. This crate is empty on other targets.
#[cfg(target_os = "macos")]
mod accessibility;
#[cfg(target_os = "macos")]
mod capture;
#[cfg(target_os = "macos")]
mod ffi;
#[cfg(target_os = "macos")]
mod keyboard;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod paste;
#[cfg(target_os = "macos")]
pub use macos::MacOsBackend;
