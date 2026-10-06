//! Owned, type-checked Accessibility objects. All remote messages have a short
//! timeout so an unresponsive application cannot block playback indefinitely.
use core_foundation::{
    array::CFArray,
    base::{CFType, CFTypeID, CFTypeRef, TCFType},
    boolean::CFBoolean,
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use scriptaro_core::{ControlRole, ControlSelector};
use scriptaro_platform::{BackendError, BackendResult, ControlInfo};
use std::{
    ptr,
    time::{Duration, Instant},
};

// AXUIElement is an opaque CF object; AXError is an SDK SInt32 enum.
type AXUIElementRef = CFTypeRef;
// Real TextEdit event handling can exceed 250 ms while opening documents or
// processing text. Keep remote calls bounded without treating that latency as a
// lost target. Never retry a dispatched focus/press operation.
const MESSAGE_TIMEOUT_SECONDS: f32 = 1.0;
const MAX_WINDOWS: isize = 256;
// Real browser trees can exceed 500 ms even when every AX request succeeds.
const ENUMERATION_BUDGET: Duration = Duration::from_secs(2);

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> i32;
}

fn native(message: impl Into<String>) -> BackendError {
    BackendError::Native(message.into())
}

fn check(code: i32, operation: &str) -> BackendResult<()> {
    match code {
        0 => Ok(()),
        -25211 => Err(BackendError::PermissionDenied(
            "Accessibility access is required for window control".into(),
        )),
        -25204 => Err(native(format!(
            "{operation}: application is busy or unresponsive (Accessibility timeout)"
        ))),
        -25202 => Err(native(format!(
            "{operation}: window or application no longer exists"
        ))),
        -25205 | -25206 | -25208 => Err(native(format!(
            "{operation}: application does not expose the required Accessibility operation"
        ))),
        _ => Err(native(format!("{operation}: Accessibility error {code}"))),
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct Element(CFType);

impl Element {
    fn from_cf(value: CFType) -> BackendResult<Self> {
        // SAFETY: type-ID lookup takes no arguments and owns no resources.
        if value.type_of() != unsafe { AXUIElementGetTypeID() } {
            return Err(native("Accessibility returned an unexpected element type"));
        }
        check(
            // SAFETY: value has been checked as an AXUIElement and stays retained.
            unsafe {
                AXUIElementSetMessagingTimeout(value.as_CFTypeRef(), MESSAGE_TIMEOUT_SECONDS)
            },
            "set Accessibility timeout",
        )?;
        Ok(Self(value))
    }

    pub(crate) fn application(pid: u32) -> BackendResult<Self> {
        let pid = i32::try_from(pid)
            .ok()
            .filter(|pid| *pid > 0)
            .ok_or_else(|| native("invalid Accessibility application PID"))?;
        // SAFETY: PID is positive and representable; Create returns a +1 CF object.
        let raw = unsafe { AXUIElementCreateApplication(pid) };
        if raw.is_null() {
            return Err(native("could not create Accessibility application"));
        }
        // SAFETY: non-null Create result is owned here and released by CFType.
        Self::from_cf(unsafe { CFType::wrap_under_create_rule(raw) })
    }

    fn attribute(&self, name: &str) -> BackendResult<Option<CFType>> {
        self.read_attribute(name, false)
    }

    fn read_attribute(&self, name: &str, optional: bool) -> BackendResult<Option<CFType>> {
        let attribute = CFString::new(name);
        let mut raw: CFTypeRef = ptr::null();
        // SAFETY: both inputs are retained CF objects; out-pointer is writable.
        let code = unsafe {
            AXUIElementCopyAttributeValue(
                self.0.as_CFTypeRef(),
                attribute.as_concrete_TypeRef(),
                &mut raw,
            )
        };
        let value = if raw.is_null() {
            None
        } else {
            // SAFETY: a non-null Copy result is a +1 CF object; always balance it.
            Some(unsafe { CFType::wrap_under_create_rule(raw) })
        };
        if code == -25212 || (optional && code == -25205) {
            return Ok(None);
        } // kAXErrorNoValue: absent, not unsupported.
        // Cocoa can advertise a label attribute but return kAXErrorFailure for
        // an unlabeled text control. Preserve "unknown" separately from absence.
        if optional && code == -25200 && matches!(name, "AXTitle" | "AXDescription") {
            return Err(BackendError::ControlLabelUnavailable);
        }
        check(code, name)?;
        value.map(Some).ok_or_else(|| {
            native(format!(
                "{name}: Accessibility returned no value on success"
            ))
        })
    }

    fn optional_string(&self, name: &str) -> BackendResult<Option<String>> {
        self.read_attribute(name, true)?
            .map(|value| {
                value
                    .downcast::<CFString>()
                    .map(|s| s.to_string())
                    .ok_or_else(|| native("Accessibility metadata is not a string"))
            })
            .transpose()
            .map(|value| value.filter(|s| !s.is_empty()))
    }

    /// Explicit assertion read; discovery never calls this method.
    pub(crate) fn text_equals(&self, expected: &str) -> BackendResult<bool> {
        if self.optional_string("AXSubrole")?.as_deref() == Some("AXSecureTextField") {
            return Err(native("secure field values are unavailable"));
        }
        let value = self
            .attribute("AXValue")?
            .ok_or_else(|| native("control has no text value"))?;
        text_value_equals(&value, expected)
    }
    pub(crate) fn checked_equals(&self, expected: bool) -> BackendResult<bool> {
        let value = self
            .attribute("AXValue")?
            .ok_or_else(|| native("check box has no value"))?;
        checked_value_equals(&value, expected)
    }

    pub(crate) fn enabled(&self) -> BackendResult<bool> {
        if let Some(value) = self.read_attribute("AXEnabled", true)? {
            return value
                .downcast::<CFBoolean>()
                .map(bool::from)
                .ok_or_else(|| native("Accessibility enabled state is not a boolean"));
        }
        // NSTextView commonly omits AXEnabled. For a text area only, require
        // positive editability evidence; never assume an unsupported state is true.
        // This queries writability without reading or modifying the field contents.
        if self.optional_string("AXRole")?.as_deref() == Some("AXTextArea") {
            let attribute = CFString::new("AXValue");
            let mut settable: u8 = 0;
            check(
                // SAFETY: retained AX/CFString inputs; SDK Boolean is an unsigned byte.
                unsafe {
                    AXUIElementIsAttributeSettable(
                        self.0.as_CFTypeRef(),
                        attribute.as_concrete_TypeRef(),
                        &mut settable,
                    )
                },
                "query text area editability",
            )?;
            return Ok(settable != 0);
        }
        Err(native("control exposes no enabled state"))
    }

    pub(crate) fn focused_control(&self) -> BackendResult<Option<Element>> {
        self.attribute("AXFocusedUIElement")?
            .map(Self::from_cf)
            .transpose()
    }

    pub(crate) fn focus(&self) -> BackendResult<()> {
        let attribute = CFString::new("AXFocused");
        let value = CFBoolean::true_value();
        check(
            // SAFETY: element, attribute and boolean are retained CF objects of the SDK types.
            unsafe {
                AXUIElementSetAttributeValue(
                    self.0.as_CFTypeRef(),
                    attribute.as_concrete_TypeRef(),
                    value.as_CFTypeRef(),
                )
            },
            "focus selected control",
        )
    }

    pub(crate) fn press(&self) -> BackendResult<()> {
        let action = CFString::new("AXPress");
        check(
            // SAFETY: element and action are retained for the duration of this call.
            unsafe {
                AXUIElementPerformAction(self.0.as_CFTypeRef(), action.as_concrete_TypeRef())
            },
            "invoke selected control",
        )
    }

    /// Complete bounded traversal: never return a partial result that could hide
    /// an ambiguous match. Optional metadata and leaf children may be unsupported;
    /// communication, permission and invalid-object errors always propagate.
    pub(crate) fn controls(&self) -> BackendResult<Vec<(Element, ControlInfo)>> {
        let started = Instant::now();
        let mut queue = std::collections::VecDeque::from([(self.clone(), 0usize)]);
        let mut seen = Vec::new();
        let mut controls = Vec::new();
        while let Some((element, depth)) = queue.pop_front() {
            if started.elapsed() >= ENUMERATION_BUDGET || seen.len() >= 2048 || depth > 64 {
                return Err(native(
                    "control discovery exceeded its 2 s / 2048 element / 64 level limit; refusing an incomplete search",
                ));
            }
            if seen.contains(&element) {
                continue;
            }
            seen.push(element.clone());
            let role = match element.optional_string("AXRole")?.as_deref() {
                Some("AXTextField") => Some(ControlRole::TextField),
                Some("AXTextArea") => Some(ControlRole::TextArea),
                Some("AXButton") => Some(ControlRole::Button),
                Some("AXCheckBox") => Some(ControlRole::CheckBox),
                Some("AXComboBox") => Some(ControlRole::ComboBox),
                _ => None,
            };
            if let Some(role) = role {
                if element.optional_string("AXSubrole")?.as_deref() != Some("AXSecureTextField") {
                    let identifier = element.optional_string("AXIdentifier")?;
                    let label_result =
                        element
                            .optional_string("AXTitle")
                            .and_then(|title| match title {
                                Some(title) => Ok(Some(title)),
                                None => element.optional_string("AXDescription"),
                            });
                    let (label, label_available) = match label_result {
                        Ok(label) => (label, true),
                        Err(BackendError::ControlLabelUnavailable) => (None, false),
                        Err(error) => return Err(error),
                    };
                    controls.push((
                        element.clone(),
                        ControlInfo {
                            role,
                            identifier,
                            label,
                            label_available,
                        },
                    ));
                }
            }
            if let Some(value) = element.read_attribute("AXChildren", true)? {
                let children = value
                    .downcast::<CFArray>()
                    .ok_or_else(|| native("Accessibility children are not an array"))?;
                if children.len() as usize + seen.len() + queue.len() > 2048 {
                    return Err(native(
                        "control tree exceeds 2048 elements; refusing an incomplete search",
                    ));
                }
                for raw in children.iter() {
                    if raw.is_null() {
                        return Err(native("null Accessibility child"));
                    }
                    // SAFETY: AXChildren holds CF objects; retain under get rule, then verify AX type.
                    let child = Self::from_cf(unsafe { CFType::wrap_under_get_rule(*raw) })?;
                    queue.push_back((child, depth + 1));
                }
            }
        }
        if started.elapsed() >= ENUMERATION_BUDGET {
            return Err(native("control discovery exceeded its 2 s budget"));
        }
        Ok(controls)
    }

    pub(crate) fn find_control(
        &self,
        selector: &ControlSelector,
    ) -> BackendResult<Option<Element>> {
        let mut matched = None;
        for (element, info) in self.controls()? {
            if info.matches_metadata(selector)? {
                if matched.is_some() {
                    return Err(BackendError::AmbiguousControl);
                }
                matched = Some(element);
            }
        }
        Ok(matched)
    }

    pub(crate) fn title(&self) -> BackendResult<String> {
        match self.attribute("AXTitle")? {
            None => Ok(String::new()),
            Some(value) => value
                .downcast::<CFString>()
                .map(|value| value.to_string())
                .ok_or_else(|| native("Accessibility window title is not a string")),
        }
    }

    pub(crate) fn windows(&self) -> BackendResult<Vec<(Element, String)>> {
        let started = Instant::now();
        let Some(value) = self.attribute("AXWindows")? else {
            return Ok(Vec::new());
        };
        let array = value
            .downcast::<CFArray>()
            .ok_or_else(|| native("Accessibility window list is not an array"))?;
        if array.len() > MAX_WINDOWS {
            return Err(native(
                "application exposes more than 256 windows; refusing an incomplete search",
            ));
        }
        let mut windows = Vec::new();
        for raw in array.iter() {
            if started.elapsed() >= ENUMERATION_BUDGET {
                return Err(native(
                    "window discovery exceeded its 2 s budget; application may be unresponsive",
                ));
            }
            if raw.is_null() {
                return Err(native("Accessibility window list contains a null element"));
            }
            // SAFETY: AXWindows is a CF array of retained CF objects; the get rule
            // retains each value beyond this array's lifetime. Type is checked next.
            let element = Self::from_cf(unsafe { CFType::wrap_under_get_rule(*raw) })?;
            let title = element.title()?;
            windows.push((element, title));
        }
        if started.elapsed() >= ENUMERATION_BUDGET {
            return Err(native(
                "window discovery exceeded its 2 s budget; application may be unresponsive",
            ));
        }
        Ok(windows)
    }

    pub(crate) fn focused_window(&self) -> BackendResult<Option<Element>> {
        self.attribute("AXFocusedWindow")?
            .map(Self::from_cf)
            .transpose()
    }

    pub(crate) fn raise(&self) -> BackendResult<()> {
        if let Some(value) = self.attribute("AXMinimized")? {
            let minimized = value
                .downcast::<CFBoolean>()
                .ok_or_else(|| native("Accessibility minimized state is not a boolean"))?;
            if bool::from(minimized) {
                let attribute = CFString::new("AXMinimized");
                let value = CFBoolean::false_value();
                check(
                    // SAFETY: retained element, attribute and value have their SDK types.
                    unsafe {
                        AXUIElementSetAttributeValue(
                            self.0.as_CFTypeRef(),
                            attribute.as_concrete_TypeRef(),
                            value.as_CFTypeRef(),
                        )
                    },
                    "restore minimized window",
                )?;
            }
        }
        let action = CFString::new("AXRaise");
        check(
            // SAFETY: element and action string are retained for the duration of the call.
            unsafe {
                AXUIElementPerformAction(self.0.as_CFTypeRef(), action.as_concrete_TypeRef())
            },
            "raise selected window",
        )
    }
}

fn text_value_equals(value: &CFType, expected: &str) -> BackendResult<bool> {
    let value = value
        .downcast::<CFString>()
        .ok_or_else(|| native("control value is not text"))?;
    if value.char_len() > scriptaro_core::MAX_SCRIPT_BYTES as isize {
        return Err(native("control text exceeds 4 MiB"));
    }
    let value = value.to_string();
    if value.len() > scriptaro_core::MAX_SCRIPT_BYTES {
        return Err(native("control text exceeds 4 MiB"));
    }
    Ok(value == expected)
}

fn checked_value_equals(value: &CFType, expected: bool) -> BackendResult<bool> {
    if let Some(value) = value.downcast::<CFBoolean>() {
        return Ok(bool::from(value) == expected);
    }
    let value = value
        .downcast::<CFNumber>()
        .and_then(|n| n.to_f64())
        .ok_or_else(|| native("check box value is not numeric"))?;
    match value {
        0.0 => Ok(!expected),
        1.0 => Ok(expected),
        2.0 => Ok(false), // Mixed is neither checked nor unchecked.
        _ => Err(native("check box value is not a supported state")),
    }
}

#[cfg(test)]
mod assertion_tests {
    use super::*;
    #[test]
    fn explicit_values_preserve_empty_unicode_and_mixed_state_without_coercion() {
        for text in ["", "🦀\n", " spaced "] {
            let value = CFString::new(text).as_CFType();
            assert!(text_value_equals(&value, text).unwrap());
            assert!(!text_value_equals(&value, "different").unwrap());
            assert!(checked_value_equals(&value, true).is_err());
        }
        for (number, checked, unchecked) in
            [(0i64, false, true), (1, true, false), (2, false, false)]
        {
            let value = CFNumber::from(number).as_CFType();
            assert_eq!(checked_value_equals(&value, true).unwrap(), checked);
            assert_eq!(checked_value_equals(&value, false).unwrap(), unchecked);
            assert!(text_value_equals(&value, "").is_err());
        }
        assert!(checked_value_equals(&CFNumber::from(0.5f64).as_CFType(), false).is_err());
        assert!(checked_value_equals(&CFBoolean::true_value().as_CFType(), true).unwrap());
    }
}
