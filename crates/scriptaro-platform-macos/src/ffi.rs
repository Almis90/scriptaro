//! Small ABI surface not exposed by core-graphics. Signatures match SDK headers.
use core_graphics::{event::CGEvent, event_source::CGEventSourceStateID};
use foreign_types::ForeignType;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    // Carbon Boolean is an unsigned byte, not an Objective-C BOOL.
    pub fn AXIsProcessTrusted() -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSetTimestamp(event: core_graphics::sys::CGEventRef, timestamp: u64);
    pub fn CGPreflightPostEventAccess() -> bool;
    pub fn CGPreflightListenEventAccess() -> bool;
    pub fn CGEventSourceKeyState(state: CGEventSourceStateID, key: u16) -> bool;
}

pub fn accessibility_trusted() -> bool {
    // SAFETY: process-wide permission query, no pointer arguments or ownership.
    unsafe { AXIsProcessTrusted() != 0 }
}

pub fn can_post_events() -> bool {
    // SAFETY: process-wide permission query available on our minimum macOS 10.15.
    unsafe { CGPreflightPostEventAccess() }
}

pub fn can_listen() -> bool {
    // SAFETY: read-only permission query with no pointer arguments.
    unsafe { CGPreflightListenEventAccess() }
}

pub fn key_down(code: u16) -> bool {
    // SAFETY: valid state enum and a virtual key code; reads hardware state only.
    unsafe { CGEventSourceKeyState(CGEventSourceStateID::HIDSystemState, code) }
}

unsafe extern "C" {
    fn clock_gettime_nsec_np(clock_id: u32) -> u64;
}
/// Refresh preallocated drag events with uptime nanoseconds (SDK CLOCK_UPTIME_RAW=8).
pub fn stamp(event: &CGEvent) {
    // SAFETY: owned event pointer, SDK uint64 timestamp, read-only uptime clock.
    unsafe {
        CGEventSetTimestamp(event.as_ptr(), clock_gettime_nsec_np(8));
    }
}
