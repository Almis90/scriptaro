//! Plain-text clipboard staging and one-shot Command-V dispatch.
//! Never reads/snapshots existing clipboard contents or restores them on drop.
use crate::ffi;
use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode},
    event_source::CGEventSource,
};
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::{NSArray, NSString};
use scriptaro_platform::{BackendError, BackendResult, PreparedPaste};

fn native(message: &str) -> BackendError {
    BackendError::Native(message.into())
}

struct Paste {
    board: Retained<NSPasteboard>,
    revision: isize,
    down: CGEvent,
    up: CGEvent,
}
impl PreparedPaste for Paste {
    fn dispatch(self: Box<Self>) -> BackendResult<()> {
        autoreleasepool(|_| {
            if !ffi::accessibility_trusted() || !ffi::can_post_events() {
                return Err(BackendError::PermissionDenied(
                    "Accessibility access was lost before paste dispatch".into(),
                ));
            }
            // This detects ownership changes before posting, but the global
            // clipboard cannot be locked until the receiving app consumes it.
            if self.board.changeCount() != self.revision {
                return Err(native(
                    "clipboard changed before paste dispatch; no paste shortcut sent",
                ));
            }
            ffi::stamp(&self.down);
            ffi::stamp(&self.up);
            self.down.post(CGEventTapLocation::HID);
            self.up.post(CGEventTapLocation::HID);
            Ok(())
        })
    }
}

pub(crate) fn prepare(text: &str, source: CGEventSource) -> BackendResult<Box<dyn PreparedPaste>> {
    autoreleasepool(|_| {
        // Allocate both before any clipboard mutation. Failure cannot strand a
        // key or replace the user's clipboard without a ready pair of events.
        let down = CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true)
            .map_err(|_| native("could not allocate paste key-down event"))?;
        let up = CGEvent::new_keyboard_event(source, KeyCode::ANSI_V, false)
            .map_err(|_| native("could not allocate paste key-up event"))?;
        down.set_flags(CGEventFlags::CGEventFlagCommand);
        up.set_flags(CGEventFlags::CGEventFlagCommand);
        let text = NSString::from_str(text);
        // SAFETY: AppKit's immutable, process-lifetime NSString type constant.
        let kind = unsafe { NSPasteboardTypeString };
        let types = NSArray::from_slice(&[kind]);
        let board = NSPasteboard::generalPasteboard();
        // SAFETY: typed string array; nil owner because all data is supplied now.
        // declareTypes clears previous contents and declares the type required by
        // setString in one ownership change, whose revision is returned here.
        let revision = unsafe { board.declareTypes_owner(&types, None) };
        if board.changeCount() != revision {
            return Err(native(
                "clipboard changed while preparing paste; no paste shortcut sent",
            ));
        }
        if !board.setString_forType(&text, kind) {
            return Err(native(
                "could not write paste text to clipboard; clipboard may be cleared, no paste shortcut sent",
            ));
        }
        if board.changeCount() != revision {
            return Err(native(
                "clipboard changed while preparing paste; no paste shortcut sent",
            ));
        }
        Ok(Box::new(Paste {
            board,
            revision,
            down,
            up,
        }) as Box<dyn PreparedPaste>)
    })
}
