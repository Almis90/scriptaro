//! Selector construction uses the same exact metadata matching as playback.
use scriptaro_core::{AppSelector, ControlSelector, WindowSelector};
use scriptaro_platform::{ApplicationInfo, BackendError, BackendResult, ControlInfo, WindowInfo};

pub fn application(apps: &[ApplicationInfo], index: usize) -> BackendResult<AppSelector> {
    let app = apps
        .get(index)
        .ok_or_else(|| BackendError::Native("Choose an application".into()))?;
    if let Some(id) = app.identifier.as_ref().filter(|id| !id.trim().is_empty()) {
        if apps
            .iter()
            .filter(|a| a.identifier.as_ref() == Some(id))
            .count()
            == 1
        {
            return Ok(AppSelector::Identifier(id.clone()));
        }
    }
    // A PID is exact even when application names or bundle identifiers collide.
    Ok(AppSelector::Pid(app.pid))
}

pub fn window(
    app: AppSelector,
    windows: &[WindowInfo],
    index: usize,
) -> BackendResult<WindowSelector> {
    let selected = windows
        .get(index)
        .ok_or_else(|| BackendError::Native("Choose a window".into()))?;
    if selected.title.trim().is_empty() {
        return Err(BackendError::Native(
            "This window has no usable title".into(),
        ));
    }
    if windows.iter().filter(|w| w.title == selected.title).count() != 1 {
        return Err(BackendError::AmbiguousWindow);
    }
    Ok(WindowSelector {
        app,
        title: selected.title.clone(),
    })
}

pub fn control(
    window: WindowSelector,
    controls: &[ControlInfo],
    index: usize,
) -> BackendResult<ControlSelector> {
    let selected = controls
        .get(index)
        .ok_or_else(|| BackendError::Native("Choose a control".into()))?;
    let id = selected.identifier.clone().filter(|s| !s.trim().is_empty());
    let label = selected
        .label
        .clone()
        .filter(|s| selected.label_available && !s.trim().is_empty());
    let mut last_error =
        BackendError::Native("This control exposes no unique identifier or readable label".into());
    for (identifier, label) in [(id.clone(), None), (id, label.clone()), (None, label)] {
        if identifier.is_none() && label.is_none() {
            continue;
        }
        let selector = ControlSelector {
            window: window.clone(),
            role: selected.role,
            identifier,
            label,
        };
        let matches = controls.iter().try_fold(0, |count, c| {
            c.matches_metadata(&selector)
                .map(|m| count + usize::from(m))
        });
        match matches {
            Ok(1) => return Ok(selector),
            Ok(_) => last_error = BackendError::AmbiguousControl,
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

/// Format a fragment at an AppKit UTF-16 cursor position, without splitting Unicode.
/// The cursor must be at the indentation before an action or on a blank line.
pub fn insertion(
    source: &str,
    utf16_offset: usize,
    utf16_length: usize,
    snippet: &str,
) -> Result<String, &'static str> {
    fn byte_offset(source: &str, offset: usize) -> Result<usize, &'static str> {
        let mut units = 0;
        for (byte, ch) in source.char_indices() {
            if units == offset {
                return Ok(byte);
            }
            units += ch.len_utf16();
        }
        if units == offset {
            Ok(source.len())
        } else {
            Err("The selection splits a Unicode character or is outside the document")
        }
    }
    let start = byte_offset(source, utf16_offset)?;
    let end = byte_offset(
        source,
        utf16_offset
            .checked_add(utf16_length)
            .ok_or("Invalid selection")?,
    )?;
    let prefix = source[..start].rsplit('\n').next().unwrap_or("");
    if !prefix.chars().all(|c| c == ' ') {
        return Err(
            "Place the cursor on a blank indented line in steps, setup or reset, or select actions starting just after their indentation.",
        );
    }
    let end_prefix = source[..end].rsplit('\n').next().unwrap_or("");
    let suffix = &source[end..];
    let trailing_indent = if suffix.is_empty() || suffix.starts_with(['\n', '\r']) {
        prefix
    } else if end_prefix.chars().all(|c| c == ' ') {
        // Keep the following action's original indentation; the selection may
        // end before, inside or after its leading spaces.
        end_prefix
    } else {
        return Err("Select complete action lines, without ending inside a line.");
    };
    let mut result = snippet
        .lines()
        .collect::<Vec<_>>()
        .join(&format!("\n{prefix}"));
    result.push('\n');
    result.push_str(trailing_indent);
    Ok(result)
}
