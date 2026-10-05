// Disposable native input receiver for smoke.py. Never opens user documents.
#import <Cocoa/Cocoa.h>
#include <unistd.h>

@interface Receiver : NSObject <NSApplicationDelegate, NSTextViewDelegate, NSTextFieldDelegate>
@property NSWindow *window;
@property NSTextView *editor;
@property NSWindow *otherWindow;
@property NSTextView *otherEditor;
@property NSString *directory;
@property NSTextField *secondary;
@property BOOL simulatedBusy;
@end

@implementation Receiver
- (void)record {
    [self.editor.string writeToFile:[self.directory stringByAppendingPathComponent:@"observed.txt"]
                       atomically:YES encoding:NSUTF8StringEncoding error:NULL];
}
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    self.directory = [[[NSBundle mainBundle] bundlePath] stringByDeletingLastPathComponent];
    // Cocoa resolves Command+A through the Edit menu's responder-chain action.
    NSMenu *menu = [NSMenu new];
    NSMenuItem *edit = [NSMenuItem new];
    edit.submenu = [[NSMenu alloc] initWithTitle:@"Edit"];
    [edit.submenu addItemWithTitle:@"Select All" action:@selector(selectAll:) keyEquivalent:@"a"];
    NSMenuItem *rename = [edit.submenu addItemWithTitle:@"Rename test window" action:@selector(renameTestWindow:) keyEquivalent:@"r"];
    rename.target = self;
    NSMenuItem *switchWindow = [edit.submenu addItemWithTitle:@"Switch test window" action:@selector(switchTestWindow:) keyEquivalent:@"l"];
    switchWindow.target = self;
    NSMenuItem *duplicate = [edit.submenu addItemWithTitle:@"Duplicate test title" action:@selector(duplicateTestTitle:) keyEquivalent:@"d"];
    duplicate.target = self;
    NSMenuItem *field = [edit.submenu addItemWithTitle:@"Switch field" action:@selector(switchField:) keyEquivalent:@"g"];
    field.target = self;
    [menu addItem:edit];
    NSApp.mainMenu = menu;
    self.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(200, 200, 640, 300)
                     styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
                     backing:NSBackingStoreBuffered defer:NO];
    self.window.title = @"Scriptaro — disposable input test";
    NSView *container = self.window.contentView;
    self.editor = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 60, 640, 240)];
    self.editor.accessibilityIdentifier = @"notes-editor";
    self.secondary = [[NSTextField alloc] initWithFrame:NSMakeRect(10, 15, 220, 26)];
    self.secondary.accessibilityIdentifier = @"secondary-field";
    self.secondary.delegate = self;
    [container addSubview:self.secondary];
    for (int i = 0; i < 2; i++) {
        NSButton *button = [NSButton buttonWithTitle:@"Duplicate label" target:self action:@selector(invokeTest:)];
        button.frame = NSMakeRect(240 + i * 195, 12, 185, 32);
        button.accessibilityIdentifier = [NSString stringWithFormat:@"button-%d", i];
        [container addSubview:button];
    }
    self.editor.richText = NO;
    self.editor.automaticQuoteSubstitutionEnabled = NO;
    self.editor.automaticDashSubstitutionEnabled = NO;
    self.editor.automaticTextReplacementEnabled = NO;
    self.editor.automaticSpellingCorrectionEnabled = NO;
    self.editor.delegate = self;
    [container addSubview:self.editor];
    [self.window makeKeyAndOrderFront:nil];
    [self.window makeFirstResponder:self.editor];
    self.otherWindow = [[NSWindow alloc] initWithContentRect:NSMakeRect(250, 250, 640, 300)
                     styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
                     backing:NSBackingStoreBuffered defer:NO];
    self.otherWindow.title = @"Other document";
    self.otherEditor = [[NSTextView alloc] initWithFrame:self.otherWindow.contentView.bounds];
    self.otherEditor.delegate = self;
    self.otherEditor.editable = NO;
    self.otherEditor.accessibilityIdentifier = @"readonly-editor";
    self.otherWindow.contentView = self.otherEditor;
    [self.otherWindow orderBack:nil];
    [[NSString stringWithFormat:@"%d", getpid()]
        writeToFile:[self.directory stringByAppendingPathComponent:@"pid"]
        atomically:YES encoding:NSUTF8StringEncoding error:NULL];
}
- (void)application:(NSApplication *)application openFiles:(NSArray<NSString *> *)filenames {
    NSString *file = filenames.firstObject;
    // This fixture accepts exactly its own seed file.
    if ([file isEqualToString:[self.directory stringByAppendingPathComponent:@"seed.txt"]]) {
        self.editor.string = [NSString stringWithContentsOfFile:file encoding:NSUTF8StringEncoding error:NULL];
        [self record];
        [self.window makeKeyAndOrderFront:nil];
        [self.window makeFirstResponder:self.editor];
        self.window.title = @"Seed ready";
        [@"opened" writeToFile:[self.directory stringByAppendingPathComponent:@"opened"]
                    atomically:YES encoding:NSUTF8StringEncoding error:NULL];
        [application replyToOpenOrPrint:NSApplicationDelegateReplySuccess];
    } else {
        [application replyToOpenOrPrint:NSApplicationDelegateReplyFailure];
    }
}
- (void)invokeTest:(id)sender {
    [@"invoked" writeToFile:[self.directory stringByAppendingPathComponent:@"invoked"]
                atomically:YES encoding:NSUTF8StringEncoding error:NULL];
}
- (void)switchField:(id)sender { [self.window makeFirstResponder:self.secondary]; }
- (void)controlTextDidChange:(NSNotification *)notification {
    [self.secondary.stringValue writeToFile:[self.directory stringByAppendingPathComponent:@"secondary-observed.txt"]
                               atomically:YES encoding:NSUTF8StringEncoding error:NULL];
}
- (void)renameTestWindow:(id)sender { self.window.title = @"Renamed notes"; }
- (void)switchTestWindow:(id)sender {
    [self.otherWindow makeKeyAndOrderFront:nil];
    [self.otherWindow makeFirstResponder:self.otherEditor];
}
- (void)duplicateTestTitle:(id)sender {
    self.otherWindow.title = self.window.title;
    [@"ready" writeToFile:[self.directory stringByAppendingPathComponent:@"duplicate-ready"]
               atomically:YES encoding:NSUTF8StringEncoding error:NULL];
}
- (void)textDidChange:(NSNotification *)notification {
    if (!self.simulatedBusy && notification.object == self.editor) {
        self.simulatedBusy = YES;
        // Regression: ordinary main-thread work can outlast a 250 ms AX deadline.
        [@"busy" writeToFile:[self.directory stringByAppendingPathComponent:@"busy-tested"]
                   atomically:YES encoding:NSUTF8StringEncoding error:NULL];
        [NSThread sleepForTimeInterval:0.35];
    }
    if (notification.object == self.otherEditor) {
        [self.otherEditor.string writeToFile:[self.directory stringByAppendingPathComponent:@"other-observed.txt"]
                                 atomically:YES encoding:NSUTF8StringEncoding error:NULL];
    } else { [self record]; }
}
@end

int main(void) {
    @autoreleasepool {
        NSApplication *app = [NSApplication sharedApplication];
        Receiver *delegate = [Receiver new];
        app.delegate = delegate;
        [app setActivationPolicy:NSApplicationActivationPolicyRegular];
        [app run];
    }
}
