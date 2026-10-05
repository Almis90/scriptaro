// Test-only AX inspection and physical Quartz clicks. Never used by the engine.
#import <Cocoa/Cocoa.h>
#import <ApplicationServices/ApplicationServices.h>

static id attribute(AXUIElementRef element, CFStringRef key) {
    CFTypeRef value = NULL;
    CFAbsoluteTime started = CFAbsoluteTimeGetCurrent();
    AXError error = AXUIElementCopyAttributeValue(element, key, &value);
    if (CFAbsoluteTimeGetCurrent() - started > 0.2) fprintf(stderr, "slow %s: %d %.2f s\n", [(__bridge NSString *)key UTF8String], error, CFAbsoluteTimeGetCurrent() - started);
    if (error != kAXErrorSuccess) { if (value) CFRelease(value); return nil; }
    return CFBridgingRelease(value);
}
static NSArray *tree(AXUIElementRef root) {
    NSMutableArray *queue = [NSMutableArray arrayWithObject:(__bridge id)root];
    NSMutableArray *seen = [NSMutableArray array];
    for (NSUInteger i = 0; i < queue.count; ++i) {
        if (queue.count > 2048) @throw @"AX tree too large";
        id element = queue[i];
        if ([seen containsObject:element]) continue;
        [seen addObject:element];
        AXUIElementSetMessagingTimeout((__bridge AXUIElementRef)element, 1.0);
        id children = attribute((__bridge AXUIElementRef)element, kAXChildrenAttribute);
        if ([children isKindOfClass:NSArray.class]) [queue addObjectsFromArray:children];
    }
    return seen;
}
static NSArray *windowTree(AXUIElementRef app) {
    NSMutableArray *items = [NSMutableArray array];
    for (id window in attribute(app, kAXWindowsAttribute)) [items addObjectsFromArray:tree((__bridge AXUIElementRef)window)];
    return items;
}
static id find(AXUIElementRef root, NSString *role, NSString *text) {
    NSMutableArray *found = [NSMutableArray array];
    for (id item in ([role isEqual:@"AXMenuItem"] ? tree(root) : windowTree(root))) {
        AXUIElementRef e = (__bridge AXUIElementRef)item;
        if (![attribute(e, kAXRoleAttribute) isEqual:role]) continue;
        if ([attribute(e, kAXTitleAttribute) isEqual:text] ||
            [attribute(e, kAXValueAttribute) isEqual:text] ||
            [attribute(e, kAXDescriptionAttribute) isEqual:text]) [found addObject:item];
    }
    if (found.count > 1) @throw @"ambiguous test control";
    return found.firstObject;
}
static void mouse(id item) {
    if (!item) @throw @"test button missing";
    AXUIElementRef e = (__bridge AXUIElementRef)item;
    if (![attribute(e, kAXEnabledAttribute) boolValue]) @throw @"test button disabled";
    id p = attribute(e, kAXPositionAttribute), s = attribute(e, kAXSizeAttribute);
    CGPoint point; CGSize size;
    if (!p || !s || !AXValueGetValue((__bridge AXValueRef)p, kAXValueCGPointType, &point) ||
        !AXValueGetValue((__bridge AXValueRef)s, kAXValueCGSizeType, &size)) @throw @"missing button geometry";
    point.x += size.width / 2; point.y += size.height / 2;
    if (!CGPreflightPostEventAccess()) @throw @"test driver cannot post mouse events";
    fprintf(stderr, "click %.1f %.1f\n", point.x, point.y);
    pid_t expectedPID = 0, hitPID = 0;
    AXUIElementGetPid(e, &expectedPID);
    AXUIElementRef system = AXUIElementCreateSystemWide(), hit = NULL;
    AXUIElementSetMessagingTimeout(system, 1.0);
    AXError hitError = AXUIElementCopyElementAtPosition(system, point.x, point.y, &hit);
    if (hit) { AXUIElementGetPid(hit, &hitPID); CFRelease(hit); }
    CFRelease(system);
    if (hitError || hitPID != expectedPID) @throw [NSString stringWithFormat:@"test button covered: expected PID %d, hit PID %d, AX error %d", expectedPID, hitPID, hitError];
    CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStatePrivate);
    CGEventRef move = CGEventCreateMouseEvent(source, kCGEventMouseMoved, point, kCGMouseButtonLeft);
    CGEventRef down = CGEventCreateMouseEvent(source, kCGEventLeftMouseDown, point, kCGMouseButtonLeft);
    CGEventRef up = CGEventCreateMouseEvent(source, kCGEventLeftMouseUp, point, kCGMouseButtonLeft);
    CGEventSetIntegerValueField(down, kCGMouseEventClickState, 1);
    CGEventSetIntegerValueField(up, kCGMouseEventClickState, 1);
    CGEventSetFlags(move, 0); CGEventSetFlags(down, 0); CGEventSetFlags(up, 0);
    CGEventPost(kCGHIDEventTap, move);
    [NSThread sleepForTimeInterval:0.1];
    CGEventPost(kCGHIDEventTap, down);
    [NSThread sleepForTimeInterval:0.05];
    CGEventPost(kCGHIDEventTap, up);
    CFRelease(move); CFRelease(down); CFRelease(up); CFRelease(source);
}
int main(int argc, const char **argv) { @autoreleasepool {
    @try {
        if (argc < 3) @throw @"usage: driver PID snapshot|click|pick [arguments]";
        AXUIElementRef app = AXUIElementCreateApplication(atoi(argv[1]));
        AXUIElementSetMessagingTimeout(app, 1.0);
        NSString *command = @(argv[2]);
        if ([command isEqual:@"click"] && argc == 4) {
            mouse(find(app, @"AXButton", @(argv[3])));
        } else if ([command isEqual:@"pick"] && argc == 5) {
            if (!(([@(argv[3]) isEqual:@"Simulation"] && [@(argv[4]) isEqual:@"Desktop playback"]) ||
                  ([@(argv[3]) isEqual:@"All sections"] && [@(argv[4]) isEqual:@"Take"]))) @throw @"unsupported test selection";
            id popup = find(app, @"AXPopUpButton", @(argv[3]));
            if (!popup || AXUIElementPerformAction((__bridge AXUIElementRef)popup, kAXPressAction)) @throw @"could not open recipe selector";
            id item = nil;
            for (int i = 0; i < 20 && !item; ++i) {
                [NSThread sleepForTimeInterval:0.05];
                item = find(app, @"AXMenuItem", @(argv[4]));
            }
            if (!item || AXUIElementPerformAction((__bridge AXUIElementRef)item, kAXPressAction)) @throw @"could not choose test menu item";
        } else if ([command isEqual:@"snapshot"]) {
            NSMutableArray *rows = [NSMutableArray array];
            for (id item in windowTree(app)) {
                AXUIElementRef e = (__bridge AXUIElementRef)item;
                NSString *role = attribute(e, kAXRoleAttribute);
                if (![@[@"AXButton", @"AXTextArea", @"AXPopUpButton"] containsObject:role ?: @""]) continue;
                NSMutableDictionary *row = [NSMutableDictionary dictionaryWithObject:role forKey:@"AXRole"];
                NSArray *keys = [role isEqual:@"AXButton"] ? @[@"AXTitle", @"AXEnabled"] : @[@"AXValue"];
                for (NSString *key in keys) {
                    id value = attribute(e, (__bridge CFStringRef)key);
                    if ([value isKindOfClass:NSString.class] || [value isKindOfClass:NSNumber.class]) row[key] = value;
                }
                [rows addObject:row];
            }
            id window = attribute(app, kAXFocusedWindowAttribute);
            id control = attribute(app, kAXFocusedUIElementAttribute);
            NSDictionary *result = @{@"frontmost_pid": @([NSWorkspace sharedWorkspace].frontmostApplication.processIdentifier),
                @"window": window ? (attribute((__bridge AXUIElementRef)window, kAXTitleAttribute) ?: @"") : @"",
                @"focused_role": control ? (attribute((__bridge AXUIElementRef)control, kAXRoleAttribute) ?: @"") : @"",
                @"focused_identifier": control ? (attribute((__bridge AXUIElementRef)control, CFSTR("AXIdentifier")) ?: @"") : @"",
                @"rows": rows};
            NSData *data = [NSJSONSerialization dataWithJSONObject:result options:0 error:NULL];
            puts([[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding].UTF8String);
        } else @throw @"invalid driver arguments";
        CFRelease(app);
        return 0;
    } @catch (id error) { fprintf(stderr, "%s\n", [[error description] UTF8String]); return 1; }
}}
