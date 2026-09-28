#import <Cocoa/Cocoa.h>
#import <ApplicationServices/ApplicationServices.h>
#include "system_monitor.h"

namespace {
laufey_system_event_fn handler = nullptr;
void* handler_data = nullptr;
id lock_observer = nil, sleep_observer = nil, wake_observer = nil;
void Emit(const char* event) { if (handler) handler(handler_data, event); }
}

uint32_t DnrSetSystemMonitor(laufey_system_event_fn callback, void* data) {
  void (^change)(void) = ^{
    auto* distributed = [NSDistributedNotificationCenter defaultCenter];
    auto* workspace = [[NSWorkspace sharedWorkspace] notificationCenter];
    if (lock_observer) [distributed removeObserver:lock_observer];
    if (sleep_observer) [workspace removeObserver:sleep_observer];
    if (wake_observer) [workspace removeObserver:wake_observer];
    lock_observer = sleep_observer = wake_observer = nil;
    handler = callback; handler_data = data;
    if (!callback) return;
    lock_observer = [distributed addObserverForName:@"com.apple.screenIsLocked" object:nil
        queue:[NSOperationQueue mainQueue] usingBlock:^(NSNotification*) { Emit("lock"); }];
    sleep_observer = [workspace addObserverForName:NSWorkspaceWillSleepNotification object:nil
        queue:[NSOperationQueue mainQueue] usingBlock:^(NSNotification*) { Emit("suspend"); }];
    wake_observer = [workspace addObserverForName:NSWorkspaceDidWakeNotification object:nil
        queue:[NSOperationQueue mainQueue] usingBlock:^(NSNotification*) { Emit("resume"); }];
  };
  if ([NSThread isMainThread]) change(); else dispatch_sync(dispatch_get_main_queue(), change);
  return callback ? 15 : 0;
}

double DnrSystemIdleSeconds() {
  return CGEventSourceSecondsSinceLastEventType(kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType);
}
