use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::ProtocolObject,
    sel,
};
use objc2_app_kit::*;
use objc2_foundation::{
    NSDate, NSDefaultRunLoopMode, NSObject, NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize,
    NSString,
};
use scriptaro_desktop::{Document, Session};
#[path = "macos_builder.rs"]
mod builder_ui;
#[path = "macos_picker.rs"]
mod picker_ui;
use scriptaro_engine::{ControlState, Engine, RunOptions};
use scriptaro_platform::{DesktopBackend, recording::RecordingBackend};
use std::{
    cell::RefCell, collections::VecDeque, error::Error, path::PathBuf, rc::Rc, time::Duration,
};

define_class!(
    // SAFETY: NSView subclass only draws its background on the owning main thread.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct Background;
    impl Background {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty: NSRect) {
            NSColor::windowBackgroundColor().setFill();
            NSRectFill(dirty);
        }
    }
);
impl Background {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        // SAFETY: NSView's designated initializer takes an NSRect and returns an owned instance.
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

#[derive(Clone, Copy)]
enum Command {
    New,
    Save,
    SaveAs,
    Validate,
    Edit,
    Plan,
    Pick,
    Build,
    Open,
    Reload,
    Play,
    Retake,
    Pause,
    Resume,
    Stop,
    Quit,
}
#[derive(Default)]
struct Actions {
    queue: RefCell<VecDeque<Command>>,
}

define_class!(
    // SAFETY: NSObject imposes no subclassing requirements; this class has no Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Actions]
    struct Target;
    // SAFETY: NSObjectProtocol has no additional requirements.
    unsafe impl NSObjectProtocol for Target {}
    impl Target {
        #[unsafe(method(saveAs:))]
        fn save_as(&self, _: &NSObject) { self.push(Command::SaveAs); }
        #[unsafe(method(save:))]
        fn save(&self, _: &NSObject) { self.push(Command::Save); }
        #[unsafe(method(validate:))]
        fn validate(&self, _: &NSObject) { self.push(Command::Validate); }
        #[unsafe(method(editScript:))]
        fn edit_script(&self, _: &NSObject) { self.push(Command::Edit); }
        #[unsafe(method(showPlan:))]
        fn show_plan(&self, _: &NSObject) { self.push(Command::Plan); }
        #[unsafe(method(buildActions:))]
        fn build_actions(&self, _: &NSObject) { self.push(Command::Build); }
        #[unsafe(method(pickTarget:))]
        fn pick_target(&self, _: &NSObject) { self.push(Command::Pick); }
        #[unsafe(method(newScript:))]
        fn new_script(&self, _: &NSObject) { self.push(Command::New); }
        #[unsafe(method(open:))]
        fn open(&self, _: &NSObject) { self.push(Command::Open); }
        #[unsafe(method(reload:))]
        fn reload(&self, _: &NSObject) { self.push(Command::Reload); }
        #[unsafe(method(play:))]
        fn play(&self, _: &NSObject) { self.push(Command::Play); }
        #[unsafe(method(retake:))]
        fn retake(&self, _: &NSObject) { self.push(Command::Retake); }
        #[unsafe(method(pause:))]
        fn pause(&self, _: &NSObject) { self.push(Command::Pause); }
        #[unsafe(method(resume:))]
        fn resume(&self, _: &NSObject) { self.push(Command::Resume); }
        #[unsafe(method(quit:))]
        fn quit(&self, _: &NSObject) { self.push(Command::Quit); }
        #[unsafe(method(stop:))]
        fn stop(&self, _: &NSObject) { self.push(Command::Stop); }
    }
    // SAFETY: The delegate method follows NSApplicationDelegate's signature.
    unsafe impl NSApplicationDelegate for Target {
        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _: &NSApplication) -> NSApplicationTerminateReply {
            self.push(Command::Quit);
            NSApplicationTerminateReply::TerminateCancel
        }
    }
    // SAFETY: The delegate methods follow NSWindowDelegate's signatures.
    unsafe impl NSWindowDelegate for Target {
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self, _: &NSWindow) -> bool { self.push(Command::Quit); false }
    }
);
impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Actions::default());
        // SAFETY: NSObject init with its documented signature.
        unsafe { msg_send![super(this), init] }
    }
    fn push(&self, command: Command) {
        self.ivars().queue.borrow_mut().push_back(command);
    }
}
fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}
fn label(view: &NSView, mtm: MainThreadMarker, text: &str, frame: NSRect) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFrame(frame);
    view.addSubview(&field);
    field
}
fn button(
    view: &NSView,
    target: &Target,
    text: &str,
    action: objc2::runtime::Sel,
    frame: NSRect,
) -> Retained<NSButton> {
    // SAFETY: Target lives longer than every button and implements each passed selector with one NSObject argument.
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(text),
            Some(target),
            Some(action),
            target.mtm(),
        )
    };
    button.setFrame(frame);
    view.addSubview(&button);
    button
}

fn click(button: &NSButton) {
    // SAFETY: Every button targets the retained Target instance with a matching action signature.
    unsafe {
        button.performClick(None);
    }
}

struct Ui {
    app: Retained<NSApplication>,
    window: Retained<NSWindow>,
    transport: Retained<NSPanel>,
    target: Retained<Target>,
    path: Retained<NSTextField>,
    permissions: Retained<NSTextField>,
    status: Retained<NSTextField>,
    content: Retained<NSTextView>,
    editor: Retained<NSTextView>,
    editor_scroll: Retained<NSScrollView>,
    log_scroll: Retained<NSScrollView>,
    document_buttons: Vec<Retained<NSButton>>,
    section: Retained<NSPopUpButton>,
    mode: Retained<NSPopUpButton>,
    speed: Retained<NSTextField>,
    countdown: Retained<NSTextField>,
    idle_buttons: Vec<Retained<NSButton>>,
    retake: Retained<NSButton>,
    play: Retained<NSButton>,
    pause: Retained<NSButton>,
    resume: Retained<NSButton>,
    stop: Retained<NSButton>,
}
impl Ui {
    fn new(mtm: MainThreadMarker) -> Self {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        let target = Target::new(mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        // SAFETY: Constructed on main thread; disable release-on-close while retained in Ui.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(0., 0., 900., 640.),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: Rust owns the window's lifetime.
        unsafe {
            window.setReleasedWhenClosed(false);
        }
        window.setTitle(&NSString::from_str("Scriptaro"));
        window.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        let view = Background::new(mtm, rect(0., 0., 900., 640.));
        window.setContentView(Some(&view));
        let title = label(&view, mtm, "Scriptaro", rect(24., 580., 400., 38.));
        title.setFont(Some(&NSFont::boldSystemFontOfSize(28.)));
        label(
            &view,
            mtm,
            "Prepare a sequence. Rehearse it. Record a take.",
            rect(24., 554., 650., 24.),
        );
        let open = button(
            &view,
            &target,
            "Open script…",
            sel!(open:),
            rect(700., 580., 175., 32.),
        );
        let new_script = button(
            &view,
            &target,
            "New from recipe…",
            sel!(newScript:),
            rect(510., 580., 175., 32.),
        );
        let reload = button(
            &view,
            &target,
            "Reload",
            sel!(reload:),
            rect(760., 544., 115., 30.),
        );
        let path = label(
            &view,
            mtm,
            "Choose a YAML script to begin.",
            rect(24., 520., 850., 24.),
        );
        label(&view, mtm, "Take", rect(24., 482., 45., 24.));
        let section = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(70., 478., 310., 30.),
            false,
        );
        section.addItemWithTitle(&NSString::from_str("All sections"));
        view.addSubview(&section);
        let mode = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            rect(400., 478., 230., 30.),
            false,
        );
        for name in ["Simulation", "Desktop playback"] {
            mode.addItemWithTitle(&NSString::from_str(name));
        }
        view.addSubview(&mode);
        label(&view, mtm, "Speed", rect(650., 482., 55., 24.));
        let speed = NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(710., 482., 60., 24.));
        speed.setStringValue(&NSString::from_str("1"));
        view.addSubview(&speed);
        label(&view, mtm, "×", rect(780., 482., 25., 24.));
        label(
            &view,
            mtm,
            "Countdown (seconds)",
            rect(24., 438., 180., 24.),
        );
        let countdown =
            NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(204., 438., 70., 24.));
        countdown.setStringValue(&NSString::from_str("3"));
        view.addSubview(&countdown);
        let play = button(
            &view,
            &target,
            "Play",
            sel!(play:),
            rect(310., 432., 120., 34.),
        );
        let retake = button(
            &view,
            &target,
            "Reset + retake",
            sel!(retake:),
            rect(440., 432., 170., 34.),
        );
        let build = button(
            &view,
            &target,
            "Build actions…",
            sel!(buildActions:),
            rect(644., 432., 230., 34.),
        );
        let permissions = label(&view, mtm, "", rect(24., 394., 850., 28.));
        let status = label(&view, mtm, "Ready", rect(24., 358., 850., 28.));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(24., 54., 852., 250.));
        scroll.setHasVerticalScroller(true);
        let content = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0., 0., 828., 250.));
        content.setEditable(false);
        content.setRichText(false);
        content.setFont(Some(
            &NSFont::userFixedPitchFontOfSize(13.).expect("system monospace font"),
        ));
        scroll.setDocumentView(Some(&content));
        view.addSubview(&scroll);
        let editor_scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(24., 54., 852., 250.));
        editor_scroll.setHasVerticalScroller(true);
        let editor = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0., 0., 828., 250.));
        editor.setRichText(false);
        editor.setAllowsUndo(true);
        editor.setSmartInsertDeleteEnabled(false);
        editor.setAutomaticTextCompletionEnabled(false);
        editor.setAutomaticQuoteSubstitutionEnabled(false);
        editor.setAutomaticDashSubstitutionEnabled(false);
        editor.setAutomaticTextReplacementEnabled(false);
        editor.setAutomaticSpellingCorrectionEnabled(false);
        editor.setContinuousSpellCheckingEnabled(false);
        editor.setAutomaticLinkDetectionEnabled(false);
        editor.setAutomaticDataDetectionEnabled(false);
        editor.setFont(Some(
            &NSFont::userFixedPitchFontOfSize(13.).expect("system monospace font"),
        ));
        editor.setVerticallyResizable(true);
        editor.setMinSize(NSSize::new(828., 250.));
        editor.setMaxSize(NSSize::new(828., f64::MAX));
        editor_scroll.setDocumentView(Some(&editor));
        view.addSubview(&editor_scroll);
        scroll.setHidden(true);
        button(
            &view,
            &target,
            "Edit script",
            sel!(editScript:),
            rect(24., 316., 130., 30.),
        );
        button(
            &view,
            &target,
            "Plan & log",
            sel!(showPlan:),
            rect(164., 316., 130., 30.),
        );
        let save_as = button(
            &view,
            &target,
            "Save as…",
            sel!(saveAs:),
            rect(304., 316., 120., 30.),
        );
        let validate = button(
            &view,
            &target,
            "Validate",
            sel!(validate:),
            rect(434., 316., 110., 30.),
        );
        let save = button(
            &view,
            &target,
            "Save",
            sel!(save:),
            rect(554., 316., 110., 30.),
        );
        let pick = button(
            &view,
            &target,
            "Pick target…",
            sel!(pickTarget:),
            rect(674., 316., 200., 30.),
        );

        label(
            &view,
            mtm,
            "Simulation assumes readiness. Desktop playback operates your applications.",
            rect(24., 16., 850., 24.),
        );
        // A nonactivating transport keeps Pause/Resume/Stop from stealing target focus.
        // Main-thread panel construction with release-on-close disabled below.
        let transport = {
            NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                rect(40., 80., 360., 62.),
                NSWindowStyleMask::Titled | NSWindowStyleMask::NonactivatingPanel,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: Rust retains the panel until shutdown.
        unsafe {
            transport.setReleasedWhenClosed(false);
        }
        transport.setTitle(&NSString::from_str("Scriptaro • Playback"));
        transport.setFloatingPanel(true);
        // Keep the transport above normal windows in the target application too.
        transport.setLevel(NSFloatingWindowLevel);
        transport.setHidesOnDeactivate(false);
        transport.setBecomesKeyOnlyIfNeeded(true);
        let bar = transport.contentView().expect("panel content view");
        let pause = button(
            &bar,
            &target,
            "Pause",
            sel!(pause:),
            rect(12., 14., 102., 32.),
        );
        let resume = button(
            &bar,
            &target,
            "Resume",
            sel!(resume:),
            rect(124., 14., 102., 32.),
        );
        let stop = button(
            &bar,
            &target,
            "Stop",
            sel!(stop:),
            rect(236., 14., 110., 32.),
        );
        let menu = NSMenu::new(mtm);
        let app_item = NSMenuItem::new(mtm);
        let app_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Scriptaro"));
        // SAFETY: The retained Target implements quit: with the documented action signature.
        unsafe {
            let quit = app_menu.addItemWithTitle_action_keyEquivalent(
                &NSString::from_str("Quit Scriptaro"),
                Some(sel!(quit:)),
                &NSString::from_str("q"),
            );
            quit.setTarget(Some(&target));
        }
        app_item.setSubmenu(Some(&app_menu));
        menu.addItem(&app_item);
        let file_item = NSMenuItem::new(mtm);
        let file_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("File"));
        // SAFETY: Target is retained and implements save: with one object argument.
        unsafe {
            let save = file_menu.addItemWithTitle_action_keyEquivalent(
                &NSString::from_str("Save"),
                Some(sel!(save:)),
                &NSString::from_str("s"),
            );
            save.setTarget(Some(&target));
        }
        file_item.setSubmenu(Some(&file_menu));
        menu.addItem(&file_item);
        let edit_item = NSMenuItem::new(mtm);
        let edit = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Edit"));
        for (title, action, key) in [
            ("Undo", sel!(undo:), "z"),
            ("Redo", sel!(redo:), "Z"),
            ("Cut", sel!(cut:), "x"),
            ("Copy", sel!(copy:), "c"),
            ("Paste", sel!(paste:), "v"),
            ("Select All", sel!(selectAll:), "a"),
        ] {
            // SAFETY: Standard AppKit responder-chain actions have the documented one-object signatures.
            unsafe {
                edit.addItemWithTitle_action_keyEquivalent(
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(key),
                );
            }
        }
        edit_item.setSubmenu(Some(&edit));
        menu.addItem(&edit_item);
        app.setMainMenu(Some(&menu));
        app.finishLaunching();
        window.center();
        window.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        Self {
            app,
            window,
            transport,
            target,
            path,
            permissions,
            status,
            content,
            editor,
            editor_scroll,
            log_scroll: scroll,
            document_buttons: vec![save, save_as, validate, pick, build],
            section,
            mode,
            speed,
            countdown,
            idle_buttons: vec![new_script, open, reload],
            retake,
            play,
            pause,
            resume,
            stop,
        }
    }
    fn pump(&self) {
        autoreleasepool(|_| {
            for _ in 0..64 {
                // SAFETY: Foundation's immortal default run-loop mode is a valid NSString.
                let mode = unsafe { NSDefaultRunLoopMode };
                let Some(event) = self.app.nextEventMatchingMask_untilDate_inMode_dequeue(
                    NSEventMask::Any,
                    // A past deadline only drains queued events and can starve AX
                    // requests while idle. Give AppKit a bounded run-loop slice.
                    Some(&NSDate::dateWithTimeIntervalSinceNow(0.001)),
                    mode,
                    true,
                ) else {
                    break;
                };
                self.app.sendEvent(&event);
            }
            self.app.updateWindows();
        });
    }
    fn permission_status(&self) {
        let text = match scriptaro_platform_macos::MacOsBackend::new() {
            Ok(backend) => backend
                .permissions()
                .iter()
                .map(|p| {
                    format!(
                        "{}: {}",
                        p.name,
                        if p.granted { "granted" } else { "not granted" }
                    )
                })
                .collect::<Vec<_>>()
                .join("  •  "),
            Err(error) => error.to_string(),
        };
        self.permissions.setStringValue(&NSString::from_str(&text));
    }
    fn replace_source(&self, source: &str) -> bool {
        self.show_editor(true);
        let range = NSRange::new(0, self.editor.string().to_string().encode_utf16().count());
        let replacement = NSString::from_str(source);
        let undo = self.editor.undoManager();
        if let Some(undo) = &undo {
            undo.beginUndoGrouping();
        }
        let allowed = self
            .editor
            .shouldChangeTextInRange_replacementString(range, Some(&replacement));
        if allowed {
            self.editor
                .replaceCharactersInRange_withString(range, &replacement);
            self.editor.didChangeText();
            self.editor.setSelectedRange(NSRange::new(0, 0));
            self.editor.scrollRangeToVisible(NSRange::new(0, 0));
        }
        if let Some(undo) = &undo {
            undo.endUndoGrouping();
        }
        allowed
    }
    fn show_editor(&self, editing: bool) {
        self.editor_scroll.setHidden(!editing);
        self.log_scroll.setHidden(editing);
        if editing {
            self.window.makeFirstResponder(Some(&self.editor));
        } else {
            self.window.makeFirstResponder(Some(&self.content));
        }
    }
    fn refresh_document(&self, doc: &Document) {
        self.path.setStringValue(&NSString::from_str(&format!(
            "{}{}",
            doc.path.display(),
            if doc.is_dirty() { " • Unsaved" } else { "" }
        )));
        self.window.setDocumentEdited(doc.is_dirty());
        // Preserve the selected take through intermediate invalid drafts.
        if doc.diagnostic().is_none() {
            let selected = self.section.titleOfSelectedItem().map(|s| s.to_string());
            self.section.removeAllItems();
            self.section
                .addItemWithTitle(&NSString::from_str("All sections"));
            for section in doc.sections() {
                self.section
                    .addItemWithTitle(&NSString::from_str(&section.name));
            }
            if let Some(name) = selected {
                self.section.selectItemWithTitle(&NSString::from_str(&name));
            }
        }
    }
    fn sync_editor(&self, document: &mut Option<Document>) {
        if let Some(doc) = document {
            let source = self.editor.string().to_string();
            if source != doc.source() {
                doc.update(source);
                self.refresh_document(doc);
            }
        }
    }
    fn save(&self, document: &mut Option<Document>, session: &mut Session) -> bool {
        let Some(doc) = document else {
            return true;
        };
        match doc.save() {
            Ok(()) => {
                self.refresh_document(doc);
                session.note(if doc.diagnostic().is_some() {
                    "Draft saved; fix validation errors before playback."
                } else {
                    "Script saved."
                });
                true
            }
            Err(error) => {
                picker_ui::message(self.target.mtm(), "Could not save", &error.to_string());
                false
            }
        }
    }
    fn may_discard(&self, document: &mut Option<Document>, session: &mut Session) -> bool {
        if !document.as_ref().is_some_and(Document::is_dirty) {
            return true;
        }
        let alert = NSAlert::new(self.target.mtm());
        alert.setMessageText(&NSString::from_str("Save your changes?"));
        alert.setInformativeText(&NSString::from_str(
            "Keep this draft by saving it before continuing.",
        ));
        for title in ["Save", "Cancel", "Discard changes"] {
            alert.addButtonWithTitle(&NSString::from_str(title));
        }
        match alert.runModal() {
            1000 => self.save(document, session),
            1002 => true,
            _ => false,
        }
    }
    fn load(&self, path: PathBuf, document: &mut Option<Document>, session: &mut Session) {
        match Document::load(&path) {
            Ok(loaded) => {
                self.editor.setString(&NSString::from_str(loaded.source()));
                // Do not allow Undo to restore text from a different file.
                if let Some(undo) = self.editor.undoManager() {
                    undo.removeAllActions();
                }
                self.section.removeAllItems();
                self.refresh_document(&loaded);
                self.show_editor(true);
                session.log.clear();
                session.note(
                    loaded
                        .diagnostic()
                        .unwrap_or("Script validated. Choose a take and playback mode."),
                );
                *document = Some(loaded);
            }
            Err(error) => picker_ui::message(
                self.target.mtm(),
                "Could not open script",
                &error.to_string(),
            ),
        }
        self.permission_status();
    }
    fn selected<'a>(&self, doc: &'a Document) -> Option<&'a str> {
        let index = self.section.indexOfSelectedItem();
        if index <= 0 {
            None
        } else {
            doc.sections()
                .get(index as usize - 1)
                .map(|s| s.name.as_str())
        }
    }
}

fn save_preview(view: &NSView, path: &str) -> Result<(), Box<dyn Error>> {
    fn prepare_preview(view: &NSView) {
        view.setWantsLayer(false);
        for child in view.subviews() {
            prepare_preview(&child);
        }
        view.display();
    }
    prepare_preview(view);
    let bitmap = view
        .bitmapImageRepForCachingDisplayInRect(view.bounds())
        .ok_or("could not allocate UI preview")?;
    view.cacheDisplayInRect_toBitmapImageRep(view.bounds(), &bitmap);
    // SAFETY: Empty options dictionary contains no invalid PNG representation properties.
    let png = unsafe {
        bitmap.representationUsingType_properties(
            NSBitmapImageFileType::PNG,
            &objc2_foundation::NSDictionary::new(),
        )
    }
    .ok_or("could not encode UI preview")?;
    std::fs::create_dir_all("target")?;
    if !png.writeToFile_atomically(&NSString::from_str(path), true) {
        return Err("could not write UI preview".into());
    }
    Ok(())
}

fn editor_smoke(
    ui: &Ui,
    document: &mut Option<Document>,
    session: &mut Session,
) -> Result<(), Box<dyn Error>> {
    let original = document
        .as_ref()
        .ok_or("smoke fixture did not load")?
        .source()
        .to_owned();
    ui.editor.setString(&NSString::from_str("version: [broken"));
    ui.sync_editor(document);
    if document.as_ref().unwrap().prepare(None, false).is_ok() {
        return Err("invalid editor draft remained playable".into());
    }
    if !ui.save(document, session) {
        return Err("invalid draft save failed".into());
    }
    let path = document.as_ref().unwrap().path.clone();
    if std::fs::read_to_string(&path)? != "version: [broken" {
        return Err("save used stale source".into());
    }
    ui.editor.setString(&NSString::from_str(&original));
    ui.sync_editor(document);
    if !document.as_ref().unwrap().is_dirty() {
        return Err("repaired source not marked dirty".into());
    }
    // Exercise the exact undo-aware replacement path used by the picker.
    let undo = ui
        .editor
        .undoManager()
        .ok_or("missing editor undo manager")?;
    undo.removeAllActions();
    undo.beginUndoGrouping();
    let replacement = NSString::from_str("# picked target preview 🦀\n");
    let range = NSRange::new(0, 0);
    if !ui
        .editor
        .shouldChangeTextInRange_replacementString(range, Some(&replacement))
    {
        return Err("editor refused insertion".into());
    }
    ui.editor
        .replaceCharactersInRange_withString(range, &replacement);
    ui.editor.didChangeText();
    undo.endUndoGrouping();
    undo.undo();
    if ui.editor.string().to_string() != original {
        return Err("picker insertion could not be undone".into());
    }
    ui.sync_editor(document);
    if !ui.save(document, session) || std::fs::read_to_string(&path)? != original {
        return Err("repaired draft save failed".into());
    }
    session.note("Editor smoke: invalid draft blocked; raw source saved; insertion undone; valid source restored.");
    Ok(())
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    let mtm = MainThreadMarker::new().ok_or("desktop interface must run on the main thread")?;
    let ui = Ui::new(mtm);
    let shared = Rc::new(RefCell::new(Session::default()));
    let mut document = None;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let smoke = args.first().is_some_and(|arg| arg == "--smoke-test");
    if let Some(path) = args.get(usize::from(smoke)) {
        let path = if smoke {
            let dir = PathBuf::from(format!("target/verification/editor-{}", std::process::id()));
            std::fs::create_dir_all(&dir)?;
            let copy = dir.join("script.yaml");
            std::fs::copy(path, &copy)?;
            copy
        } else {
            path.into()
        };
        ui.load(path, &mut document, &mut shared.borrow_mut());
    }
    if smoke {
        editor_smoke(&ui, &mut document, &mut shared.borrow_mut())?;
        builder_ui::smoke(mtm)?;
        let source = document.as_ref().unwrap().source().to_owned();
        let mut builder = scriptaro_desktop::builder::Builder::new(&source)?;
        let list = builder.lists()[0].0;
        builder.edit(
            list,
            scriptaro_desktop::builder::Edit::Insert {
                index: 0,
                actions: vec![scriptaro_core::Action::Wait { duration_ms: 123 }],
            },
        )?;
        let edited = builder.finish()?;
        if !ui.replace_source(&edited) {
            return Err("guided edit was refused".into());
        }
        ui.sync_editor(&mut document);
        if document.as_ref().unwrap().source() != edited {
            return Err("guided edit did not update document".into());
        }
        ui.editor
            .undoManager()
            .ok_or("missing undo manager")?
            .undo();
        ui.sync_editor(&mut document);
        if document.as_ref().unwrap().source() != source {
            return Err("guided edit undo lost original source".into());
        }
    }
    ui.permission_status();
    let mut quit = false;
    let mut was_active = false;
    let mut rendered = String::new();
    let mut smoke_started = false;
    let mut smoke_stage = 0u8;
    let mut smoke_tick = tokio::time::Instant::now();
    let mut paused_steps = 0usize;
    let smoke_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut editor_tick = tokio::time::Instant::now();
    loop {
        ui.pump();
        if editor_tick.elapsed() >= Duration::from_millis(250)
            && shared.borrow().controller.is_none()
        {
            ui.sync_editor(&mut document);
            editor_tick = tokio::time::Instant::now();
        }
        if smoke && !smoke_started {
            if document.is_none() {
                return Err("smoke test requires a valid YAML script path".into());
            }
            ui.countdown.setStringValue(&NSString::from_str("0"));
            if document.as_ref().is_none_or(|doc| {
                doc.sections()
                    .first()
                    .is_none_or(|section| section.reset.is_none())
            }) {
                return Err("UI smoke requires a section fixture with an explicit reset; use examples/sections.yaml".into());
            }
            click(&ui.play);
            smoke_started = true;
            smoke_stage = 1;
            smoke_tick = tokio::time::Instant::now();
        }
        if smoke && smoke_tick.elapsed() >= Duration::from_millis(100) {
            match smoke_stage {
                1 if shared.borrow().controller.is_some() => {
                    click(&ui.pause);
                    smoke_stage = 2;
                    smoke_tick = tokio::time::Instant::now();
                    paused_steps = shared.borrow().completed;
                }
                2 => {
                    let session = shared.borrow();
                    if session
                        .controller
                        .as_ref()
                        .is_none_or(|c| c.state() != ControlState::Paused)
                        || session.completed != paused_steps
                    {
                        return Err("UI Pause did not suspend playback".into());
                    }
                    drop(session);
                    click(&ui.resume);
                    smoke_stage = 3;
                    smoke_tick = tokio::time::Instant::now();
                }
                3 => {
                    if shared
                        .borrow()
                        .controller
                        .as_ref()
                        .is_none_or(|c| c.state() != ControlState::Running)
                    {
                        return Err("UI Resume did not resume playback".into());
                    }
                    click(&ui.stop);
                    smoke_stage = 4;
                }
                5 => {
                    click(&ui.retake);
                    smoke_stage = 6;
                }
                _ => {}
            }
        }
        let commands: Vec<_> = ui.target.ivars().queue.borrow_mut().drain(..).collect();
        for command in commands {
            let active = shared.borrow().controller.is_some();
            if !active {
                ui.sync_editor(&mut document);
            }
            match command {
                Command::Edit => ui.show_editor(true),
                Command::Plan => ui.show_editor(false),
                Command::SaveAs if !active => {
                    if let Some(doc) = &mut document {
                        let panel = NSSavePanel::savePanel(mtm);
                        panel.setTitle(Some(&NSString::from_str("Save draft as a new file")));
                        panel.setNameFieldStringValue(&NSString::from_str("scriptaro-copy.yaml"));
                        if panel.runModal() == 1 {
                            if let Some(path) = panel.URL().and_then(|url| url.path()) {
                                match doc.save_as(&PathBuf::from(path.to_string())) {
                                    Ok(()) => {
                                        ui.refresh_document(doc);
                                        shared.borrow_mut().note("Draft saved to a new file.");
                                    }
                                    Err(error) => picker_ui::message(
                                        mtm,
                                        "Could not save as a new file",
                                        &format!("Existing files are preserved. {error}"),
                                    ),
                                }
                            }
                        }
                    }
                }
                Command::Save if !active => {
                    ui.save(&mut document, &mut shared.borrow_mut());
                }
                Command::Validate if !active => {
                    if let Some(doc) = &document {
                        shared
                            .borrow_mut()
                            .note(doc.diagnostic().unwrap_or("Script validated."));
                    }
                    ui.show_editor(false);
                }
                Command::Build if !active && document.is_some() => {
                    let source = document.as_ref().unwrap().source().to_owned();
                    match builder_ui::build(mtm, &source) {
                        Ok(Some(source)) => {
                            if ui.replace_source(&source) {
                                ui.sync_editor(&mut document);
                                shared.borrow_mut().note("Guided edits applied to the draft. Save when ready; Undo restores the original source.");
                            }
                        }
                        Ok(None) => {}
                        Err(error) => picker_ui::message(
                            mtm,
                            "Could not open action builder",
                            &format!("Fix validation errors in Edit script first. {error}"),
                        ),
                    }
                }
                Command::Pick if !active && document.is_some() => {
                    ui.show_editor(true);
                    let range = ui.editor.selectedRange();
                    match picker_ui::pick(mtm) {
                        Ok(Some(snippet)) => {
                            match scriptaro_desktop::picker::insertion(
                                &ui.editor.string().to_string(),
                                range.location,
                                range.length,
                                &snippet,
                            ) {
                                Ok(text) => {
                                    let replacement = NSString::from_str(&text);
                                    if ui.editor.shouldChangeTextInRange_replacementString(
                                        range,
                                        Some(&replacement),
                                    ) {
                                        ui.editor.replaceCharactersInRange_withString(
                                            range,
                                            &replacement,
                                        );
                                        ui.editor.didChangeText();
                                        let cursor = NSRange::new(
                                            range.location + text.encode_utf16().count(),
                                            0,
                                        );
                                        ui.editor.setSelectedRange(cursor);
                                        ui.editor.scrollRangeToVisible(cursor);
                                        ui.sync_editor(&mut document);
                                        shared.borrow_mut().note("Target actions inserted. Review and validate before playback.");
                                    }
                                }
                                Err(error) => {
                                    picker_ui::message(mtm, "Choose an insertion point", error)
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            picker_ui::message(mtm, "Could not select target", &error.to_string())
                        }
                    }
                    ui.permission_status();
                }
                Command::Quit => {
                    if !ui.may_discard(&mut document, &mut shared.borrow_mut()) {
                        continue;
                    }
                    quit = true;
                    if let Some(control) = &shared.borrow().controller {
                        control.cancel();
                    }
                }
                Command::Pause | Command::Resume | Command::Stop => {
                    if let Some(control) = &shared.borrow().controller {
                        match command {
                            Command::Pause => control.pause(),
                            Command::Resume => control.resume(),
                            _ => control.cancel(),
                        }
                    }
                }
                Command::Open if !active => {
                    if !ui.may_discard(&mut document, &mut shared.borrow_mut()) {
                        continue;
                    }
                    let panel = NSOpenPanel::openPanel(mtm);
                    panel.setAllowsMultipleSelection(false);
                    panel.setCanChooseDirectories(false);
                    if panel.runModal() == 1 {
                        if let Some(path) = panel.URL().and_then(|url| url.path()) {
                            ui.load(
                                path.to_string().into(),
                                &mut document,
                                &mut shared.borrow_mut(),
                            );
                        }
                    }
                }
                Command::New if !active => {
                    if !ui.may_discard(&mut document, &mut shared.borrow_mut()) {
                        continue;
                    }
                    let panel = NSSavePanel::savePanel(mtm);
                    panel.setTitle(Some(&NSString::from_str("New script from recipe")));
                    panel.setNameFieldStringValue(&NSString::from_str("scriptaro.yaml"));
                    let picker = NSPopUpButton::initWithFrame_pullsDown(
                        NSPopUpButton::alloc(mtm),
                        rect(0., 0., 320., 32.),
                        false,
                    );
                    for recipe in scriptaro_core::recipes::RECIPES {
                        picker.addItemWithTitle(&NSString::from_str(recipe.id));
                    }
                    panel.setAccessoryView(Some(&picker));
                    if panel.runModal() == 1 {
                        if let (Some(path), Some(recipe)) = (
                            panel.URL().and_then(|url| url.path()),
                            scriptaro_core::recipes::RECIPES
                                .get(picker.indexOfSelectedItem() as usize),
                        ) {
                            let path = PathBuf::from(path.to_string());
                            match recipe.create(&path) {
                                Ok(()) => {
                                    ui.mode.selectItemAtIndex(0);
                                    ui.load(path, &mut document, &mut shared.borrow_mut());
                                    shared.borrow_mut().note("Starter saved. Edit here, use Pick target for actions, and rehearse in Simulation.");
                                }
                                Err(error) => shared.borrow_mut().note(format!(
                                    "Could not create starter (existing files are kept): {error}"
                                )),
                            }
                        }
                    }
                }
                Command::Reload if !active => {
                    if !ui.may_discard(&mut document, &mut shared.borrow_mut()) {
                        continue;
                    }
                    if let Some(doc) = &document {
                        ui.load(doc.path.clone(), &mut document, &mut shared.borrow_mut());
                    }
                }
                Command::Play | Command::Retake if !active && !quit => {
                    let Some(doc) = &document else {
                        continue;
                    };
                    let prepared =
                        doc.prepare(ui.selected(doc), matches!(command, Command::Retake));
                    let speed = ui.speed.stringValue().to_string().parse::<f64>();
                    let countdown = ui.countdown.stringValue().to_string().parse::<f64>();
                    let (script, speed, countdown) = match (prepared, speed, countdown) {
                        (Ok(script), Ok(speed), Ok(countdown))
                            if speed.is_finite()
                                && (0.01..=100.).contains(&speed)
                                && countdown.is_finite()
                                && (0.0..=86400.).contains(&countdown) =>
                        {
                            (script, speed, countdown)
                        }
                        (Err(error), _, _) => {
                            shared.borrow_mut().note(error.to_string());
                            continue;
                        }
                        _ => {
                            shared
                                .borrow_mut()
                                .note("Speed must be 0.01–100; countdown must be 0–86400 seconds.");
                            continue;
                        }
                    };
                    let simulated = ui.mode.indexOfSelectedItem() == 0;
                    let backend: Result<Box<dyn DesktopBackend>, _> = if simulated {
                        Ok(Box::new(RecordingBackend::default()) as Box<dyn DesktopBackend>)
                    } else {
                        scriptaro_platform_macos::MacOsBackend::new()
                            .map(|b| Box::new(b) as Box<dyn DesktopBackend>)
                    };
                    let mut backend = match backend {
                        Ok(backend) => backend,
                        Err(error) => {
                            shared.borrow_mut().note(error.to_string());
                            continue;
                        }
                    };
                    let options = RunOptions {
                        base_dir: doc.base_dir(),
                        speed,
                        initial_delay: Duration::from_secs_f64(countdown),
                        skip_delays: false,
                    };
                    // Engine and backend are owned by one local task on this GUI thread.
                    let state = shared.clone();
                    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
                    tokio::task::spawn_local(async move {
                        let engine = Engine::new(backend.as_mut(), options);
                        let mut events = engine.subscribe();
                        {
                            let mut session = state.borrow_mut();
                            session.controller = Some(engine.controller());
                            session.last_report = None;
                            session.completed = 0;
                            session.total = script.steps.len();
                            session.log.clear();
                            session.note(if simulated {
                                "Simulation: readiness assumed; no desktop effects."
                            } else {
                                "Desktop playback starting after countdown."
                            });
                        }
                        let _ = started_tx.send(());
                        let result = {
                            let run = engine.run(&script);
                            tokio::pin!(run);
                            loop {
                                tokio::select! {
                                    biased;
                                    result = &mut run => break result,
                                    event = events.recv() => if let Ok(event) = event { state.borrow_mut().event(event); },
                                }
                            }
                        };
                        let mut session = state.borrow_mut();
                        while let Ok(event) = events.try_recv() {
                            session.event(event);
                        }
                        match result {
                            Ok(report) => {
                                session.note(format!(
                                    "{:?}: {}/{} steps",
                                    report.status,
                                    report.completed_steps,
                                    script.steps.len()
                                ));
                                session.last_report = Some(report);
                            }
                            Err(error) => session.note(error.to_string()),
                        }
                        session.controller = None;
                    });
                    let _ = started_rx.await;
                    ui.permission_status();
                    ui.transport.orderFrontRegardless();
                    if !simulated {
                        ui.window.orderOut(None);
                    }
                    was_active = true;
                }
                _ => {}
            }
        }
        let active = {
            let session = shared.borrow();
            let active = session.controller.is_some();
            if was_active && !active {
                ui.transport.orderOut(None);
                if !quit {
                    ui.window.makeKeyAndOrderFront(None);
                }
                was_active = false;
                if smoke {
                    let report = session
                        .last_report
                        .as_ref()
                        .ok_or("desktop simulation did not produce a report")?;
                    match smoke_stage {
                        4 if report.status == scriptaro_engine::RunStatus::Cancelled => {
                            ui.section.selectItemAtIndex(1);
                            ui.speed.setStringValue(&NSString::from_str("10"));
                            smoke_stage = 5;
                        }
                        6 if report.status == scriptaro_engine::RunStatus::Completed => {
                            println!(
                                "Desktop smoke test passed: editor validation/save/undo; native action forms and guided edit undo; native views; Pause/Resume/Stop actions; selected section reset + retake; {} steps completed.",
                                report.completed_steps
                            );
                            quit = true;
                        }
                        _ => return Err("desktop transport or retake smoke check failed".into()),
                    }
                }
            }
            for button in &ui.idle_buttons {
                button.setEnabled(!active);
            }
            let valid = document
                .as_ref()
                .is_some_and(|doc| doc.diagnostic().is_none());
            ui.play.setEnabled(!active && valid);
            ui.editor.setEditable(!active && document.is_some());
            for button in &ui.document_buttons {
                button.setEnabled(!active && document.is_some());
            }
            ui.retake.setEnabled(
                !active
                    && document.as_ref().is_some_and(|doc| {
                        ui.selected(doc).is_some_and(|name| {
                            doc.sections()
                                .iter()
                                .any(|s| s.name == name && s.reset.is_some())
                        })
                    }),
            );
            ui.section.setEnabled(!active && valid);
            ui.mode.setEnabled(!active);
            ui.speed.setEnabled(!active);
            ui.countdown.setEnabled(!active);
            let paused = session
                .controller
                .as_ref()
                .is_some_and(|control| control.state() == ControlState::Paused);
            ui.pause.setEnabled(active && !paused);
            ui.resume.setEnabled(active && paused);
            ui.stop.setEnabled(active);
            ui.status.setStringValue(&NSString::from_str(&format!(
                "{} • {} / {} steps",
                if paused {
                    "Paused"
                } else if active {
                    "Playing"
                } else if !valid && document.is_some() {
                    "Invalid draft — Validate for details"
                } else if document.as_ref().is_some_and(Document::is_dirty) {
                    "Valid draft • Unsaved"
                } else {
                    "Ready"
                },
                session.completed,
                session.total
            )));
            let mut text = String::new();
            if !active {
                if let Some(doc) = &document {
                    if let Some(error) = doc.diagnostic() {
                        text.push_str(&format!("VALIDATION\n{error}\n"));
                    }
                    if let Ok(plan) = doc.prepare(ui.selected(doc), false) {
                        text.push_str("TAKE PLAN (setup → readiness → steps)\n");
                        for (i, action) in plan.steps.iter().enumerate().take(1000) {
                            text.push_str(&format!("{:>4}  {}\n", i + 1, action.kind()));
                        }
                        if plan.steps.len() > 1000 {
                            text.push_str("… preview limited to the first 1000 steps\n");
                        }
                    }
                }
            }
            text.push_str("\nPLAYBACK LOG\n");
            for line in &session.log {
                text.push_str(line);
                text.push('\n');
            }
            if rendered != text {
                ui.content.setString(&NSString::from_str(&text));
                rendered = text;
            }
            active
        };
        if quit && !active {
            if smoke {
                let view = ui.window.contentView().ok_or("missing content view")?;
                save_preview(&view, "target/desktop-preview.png")?;
            }
            break;
        }
        if smoke && tokio::time::Instant::now() > smoke_deadline {
            return Err("desktop smoke test timed out".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ui.app.setDelegate(None);
    ui.transport.orderOut(None);
    ui.window.orderOut(None);
    Ok(())
}
