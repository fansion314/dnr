#import <AppKit/AppKit.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int fail(NSString *message) {
  fprintf(stderr, "%s\n", message.UTF8String);
  [NSApplication sharedApplication];
  [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
  NSAlert *alert = [[NSAlert alloc] init];
  alert.messageText = @"应用无法启动";
  alert.informativeText = message;
  [alert addButtonWithTitle:@"好"];
  [alert runModal];
  return 1;
}

int main(int argc, char **argv) {
  @autoreleasepool {
    NSBundle *bundle = NSBundle.mainBundle;
    NSString *runtime = [bundle objectForInfoDictionaryKey:@"DNRRuntimePath"];
    NSString *name = [bundle objectForInfoDictionaryKey:@"CFBundleDisplayName"];
    NSString *identity = [bundle objectForInfoDictionaryKey:@"CFBundleIdentifier"];
    NSString *package = [bundle pathForResource:@"application" ofType:@"dnp"];
    NSString *icon = [bundle pathForResource:@"AppIcon" ofType:@"icns"];
    if (!package || !icon || ![runtime isKindOfClass:NSString.class])
      return fail(@"应用资源不完整，请重新安装应用。");
    if (![[NSFileManager defaultManager] isExecutableFileAtPath:runtime])
      return fail([NSString stringWithFormat:@"找不到共享 dnr 运行时，请安装到：\n%@", runtime]);
    // Preserve LaunchServices identity across exec; the backend reads these
    // before creating NSApp. No shell expansion or working-directory change.
    setenv("LAUFEY_APP_NAME", name.UTF8String, 1);
    setenv("LAUFEY_APP_ID", identity.UTF8String, 1);
    setenv("LAUFEY_APP_ICON", icon.fileSystemRepresentation, 1);
    char **args = calloc((size_t)argc + 2, sizeof(char *));
    if (!args) return fail(@"无法分配启动参数。");
    args[0] = (char *)runtime.fileSystemRepresentation;
    args[1] = (char *)package.fileSystemRepresentation;
    for (int i = 1; i < argc; i++) args[i + 1] = argv[i];
    execv(args[0], args);
    int error = errno;
    free(args);
    return fail([NSString stringWithFormat:@"无法启动 dnr：%s", strerror(error)]);
  }
}
