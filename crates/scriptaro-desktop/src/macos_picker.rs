//! AppKit presentation only; discovery and selector rules remain in shared layers.
use super::rect;
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_app_kit::*;
use objc2_foundation::NSString;
use scriptaro_core::{Action, AppSelector, ControlRole, yaml};
use scriptaro_desktop::picker;
use scriptaro_platform::DesktopBackend;
use std::error::Error;

pub fn message(mtm: MainThreadMarker, title: &str, detail: &str) {
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.addButtonWithTitle(&NSString::from_str("OK"));
    alert.runModal();
}
pub(super) fn choice(
    mtm: MainThreadMarker,
    title: &str,
    detail: &str,
    items: &[String],
    buttons: &[&str],
) -> Option<(usize, isize)> {
    if items.is_empty() {
        message(
            mtm,
            title,
            "No discoverable targets. Open the target application/window and try again.",
        );
        return None;
    }
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(detail));
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        rect(0., 0., 620., 32.),
        false,
    );
    for (index, item) in items.iter().enumerate() {
        // Ordinals keep AppKit from coalescing identical menu titles, never select by index at playback.
        popup.addItemWithTitle(&NSString::from_str(&format!("{}. {item}", index + 1)));
    }
    alert.setAccessoryView(Some(&popup));
    for title in buttons {
        alert.addButtonWithTitle(&NSString::from_str(title));
    }
    let response = alert.runModal();
    if response == 1001 {
        None
    } else {
        Some((popup.indexOfSelectedItem() as usize, response))
    }
}
fn preview(
    mtm: MainThreadMarker,
    actions: &[Action],
    transient: bool,
) -> Result<Option<String>, Box<dyn Error>> {
    let snippet = yaml::actions_to_string(actions)?;
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str("Insert target actions"));
    let detail = if transient {
        "This selection uses a process ID and must be picked again after the app restarts. Review the actions below. Insert at an indented blank line in steps, setup or reset, or replace selected actions. Nothing runs until playback."
    } else {
        "Review the actions below. Insert at an indented blank line in steps, setup or reset, or replace selected actions. Nothing runs until playback; targets are resolved again then."
    };
    alert.setInformativeText(&NSString::from_str(detail));
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0., 0., 620., 280.));
    scroll.setHasVerticalScroller(true);
    let text: Retained<NSTextView> =
        NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0., 0., 596., 280.));
    text.setEditable(false);
    text.setRichText(false);
    text.setFont(Some(
        &NSFont::userFixedPitchFontOfSize(12.).expect("system monospace font"),
    ));
    text.setString(&NSString::from_str(&snippet));
    scroll.setDocumentView(Some(&text));
    alert.setAccessoryView(Some(&scroll));
    alert.addButtonWithTitle(&NSString::from_str("Insert"));
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    Ok((alert.runModal() == 1000).then_some(snippet))
}

pub fn pick(mtm: MainThreadMarker) -> Result<Option<String>, Box<dyn Error>> {
    let Some(actions) = pick_actions(mtm)? else {
        return Ok(None);
    };
    let transient = actions.iter().any(|a| match a {
        Action::ActivateApp { app, .. } => matches!(app, AppSelector::Pid(_)),
        Action::ActivateWindow { window, .. } => matches!(window.app, AppSelector::Pid(_)),
        _ => false,
    });
    preview(mtm, &actions, transient)
}

pub(super) fn pick_actions(mtm: MainThreadMarker) -> Result<Option<Vec<Action>>, Box<dyn Error>> {
    let mut backend = scriptaro_platform_macos::MacOsBackend::new()?;
    let mut apps = backend.list_applications()?;
    apps.sort_by(|a, b| a.name.cmp(&b.name).then(a.pid.cmp(&b.pid)));
    let titles = apps
        .iter()
        .map(|a| {
            format!(
                "{} — {} (PID {})",
                a.name,
                a.identifier.as_deref().unwrap_or("no identifier"),
                a.pid
            )
        })
        .collect::<Vec<_>>();
    let Some((index, response)) = choice(
        mtm,
        "Choose an application",
        "Discovery reads target metadata only. Reopen to refresh. A process-ID target must be selected again after the app restarts.",
        &titles,
        &["Choose window…", "Cancel", "Use application"],
    ) else {
        return Ok(None);
    };
    let app = picker::application(&apps, index)?;
    if response == 1002 {
        return Ok(Some(vec![Action::ActivateApp {
            app,
            timeout_ms: None,
        }]));
    }
    let windows = backend.list_windows(&app)?;
    let titles = windows
        .iter()
        .map(|w| {
            if w.title.is_empty() {
                "(untitled)".into()
            } else {
                w.title.clone()
            }
        })
        .collect::<Vec<_>>();
    let Some((index, response)) = choice(
        mtm,
        "Choose a window",
        "Window titles must be nonempty and unique within this application.",
        &titles,
        &["Choose control…", "Cancel", "Use window"],
    ) else {
        return Ok(None);
    };
    let window = picker::window(app, &windows, index)?;
    let mut actions = vec![Action::ActivateWindow {
        window: window.clone(),
        timeout_ms: None,
    }];
    if response == 1002 {
        return Ok(Some(actions));
    }
    let controls = backend.list_controls(&window)?;
    let titles = controls
        .iter()
        .map(|c| {
            format!(
                "{:?} — id: {} — label: {}",
                c.role,
                c.identifier.as_deref().unwrap_or("—"),
                if c.label_available {
                    c.label.as_deref().unwrap_or("—")
                } else {
                    "unreadable"
                }
            )
        })
        .collect::<Vec<_>>();
    let Some((index, _)) = choice(
        mtm,
        "Choose a control",
        "Only supported Accessibility roles are shown. Controls must have unique identifiers or readable labels; field contents are never inspected.",
        &titles,
        &["Continue", "Cancel"],
    ) else {
        return Ok(None);
    };
    let control = picker::control(window, &controls, index)?;
    let invoke = if matches!(control.role, ControlRole::Button | ControlRole::CheckBox) {
        let Some((index, _)) = choice(
            mtm,
            "Choose an action",
            "Focus moves keyboard focus. Invoke presses the button or toggles the checkbox when the script runs.",
            &["Focus control".into(), "Invoke control".into()],
            &["Review YAML", "Cancel"],
        ) else {
            return Ok(None);
        };
        index == 1
    } else {
        false
    };
    actions.push(if invoke {
        Action::InvokeControl {
            control,
            timeout_ms: None,
        }
    } else {
        Action::FocusControl {
            control,
            timeout_ms: None,
        }
    });
    Ok(Some(actions))
}
