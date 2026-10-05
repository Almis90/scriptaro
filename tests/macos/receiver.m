// Disposable native input receiver for smoke.py. Never opens user documents.
#import <Cocoa/Cocoa.h>
#include <unistd.h>

@interface Receiver : NSObject <NSApplicationDelegate, NSTextViewDelegate>
@property NSWindow *window;
@property NSTextView *editor;
@property NSString *directory;
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
    [menu addItem:edit];
    NSApp.mainMenu = menu;
    self.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(200, 200, 640, 300)
                     styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
                     backing:NSBackingStoreBuffered defer:NO];
    self.window.title = @"Scriptaro — disposable input test";
    self.editor = [[NSTextView alloc] initWithFrame:self.window.contentView.bounds];
    self.editor.richText = NO;
    self.editor.automaticQuoteSubstitutionEnabled = NO;
    self.editor.automaticDashSubstitutionEnabled = NO;
    self.editor.automaticTextReplacementEnabled = NO;
    self.editor.automaticSpellingCorrectionEnabled = NO;
    self.editor.delegate = self;
    self.window.contentView = self.editor;
    [self.window makeKeyAndOrderFront:nil];
    [self.window makeFirstResponder:self.editor];
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
        [@"opened" writeToFile:[self.directory stringByAppendingPathComponent:@"opened"]
                    atomically:YES encoding:NSUTF8StringEncoding error:NULL];
        [application replyToOpenOrPrint:NSApplicationDelegateReplySuccess];
    } else {
        [application replyToOpenOrPrint:NSApplicationDelegateReplyFailure];
    }
}
- (void)textDidChange:(NSNotification *)notification { [self record]; }
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
