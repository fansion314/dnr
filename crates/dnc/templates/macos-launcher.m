#import <AppKit/AppKit.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <stdlib.h>
#include <string.h>
#include <sys/event.h>
#include <sys/wait.h>
#include <unistd.h>

extern char **environ;

// Keep LaunchServices' original process alive. Replacing it with exec changes
// its pid version, which makes MenuBarAgent reject the runtime's status items.
// The child inherits the launch environment and registers the app's GUI itself;
// the launcher must not create a second NSApplication on the successful path.
static int runRuntime(char **args) {
  const int forwarded[] = {SIGHUP, SIGINT, SIGQUIT, SIGTERM};
  enum { count = sizeof(forwarded) / sizeof(forwarded[0]) };
  struct sigaction previous[count];
  struct sigaction ignored = {.sa_handler = SIG_IGN};
  sigemptyset(&ignored.sa_mask);
  sigset_t childDefaults;
  sigemptyset(&childDefaults);
  size_t installed = 0;
  pid_t child = -1;
  int result = -1, error = 0;
  int queue = kqueue();
  if (queue < 0) return -1;
  if (fcntl(queue, F_SETFD, FD_CLOEXEC) < 0) goto failed;
  for (size_t i = 0; i < count; i++) {
    struct kevent event;
    EV_SET(&event, forwarded[i], EVFILT_SIGNAL, EV_ADD, 0, 0, NULL);
    if (kevent(queue, &event, 1, NULL, 0, NULL) < 0) goto failed;
    if (sigaction(forwarded[i], &ignored, &previous[i]) < 0) goto failed;
    installed++;
    // Preserve signals ignored by the caller; undo our own ignores in dnr.
    if (previous[i].sa_handler != SIG_IGN) sigaddset(&childDefaults, forwarded[i]);
  }
  posix_spawnattr_t attributes;
  error = posix_spawnattr_init(&attributes);
  if (error != 0) goto finished;
  error = posix_spawnattr_setsigdefault(&attributes, &childDefaults);
  if (error == 0)
    error = posix_spawnattr_setflags(&attributes, POSIX_SPAWN_SETSIGDEF);
  if (error == 0)
    error = posix_spawn(&child, args[0], NULL, &attributes, args, environ);
  posix_spawnattr_destroy(&attributes);
  if (error != 0) goto finished;

  struct kevent event;
  EV_SET(&event, child, EVFILT_PROC, EV_ADD | EV_ONESHOT, NOTE_EXIT, 0, NULL);
  // A very short-lived child may exit before registration. It remains waitable.
  if (kevent(queue, &event, 1, NULL, 0, NULL) < 0 && errno != ESRCH) goto failed;
  for (;;) {
    int status;
    pid_t waited = waitpid(child, &status, WNOHANG);
    if (waited == child) {
      child = -1;
      result = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
      break;
    }
    if (waited < 0) {
      if (errno == EINTR) continue;
      if (errno == ECHILD) child = -1;  // Never signal a PID we no longer own.
      goto failed;
    }
    // Unlike sigwait(SIGCHLD), this cannot lose the exit notification to a
    // Foundation worker thread. Signal filters also observe ignored signals.
    if (kevent(queue, NULL, 0, &event, 1, NULL) < 0) {
      if (errno == EINTR) continue;
      goto failed;
    }
    if (event.filter == EVFILT_SIGNAL) kill(child, (int)event.ident);
  }
  goto finished;
failed:
  error = errno;
finished:
  if (child > 0) {
    // If supervision itself failed, do not abandon a running app or zombie.
    kill(child, SIGKILL);
    while (waitpid(child, NULL, 0) < 0 && errno == EINTR) {}
  }
  close(queue);
  for (size_t i = 0; i < installed; i++) sigaction(forwarded[i], &previous[i], NULL);
  errno = error;
  return result;
}

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

static NSArray<NSString *> *runtimeCandidates(NSString *configured, NSString *path,
                                              NSString *home, NSString *cwd) {
  // Explicit paths are pins: do not silently substitute a different runtime.
  if (configured && ![configured isEqualToString:@"dnr"])
    return configured.isAbsolutePath ? @[configured] : @[];
  NSMutableOrderedSet<NSString *> *candidates = [NSMutableOrderedSet orderedSet];
  // Preserve POSIX PATH ordering, including relative/empty entries. Resolve
  // relative entries against the caller's cwd without changing that cwd.
  if (path) {
    for (NSString *directory in [path componentsSeparatedByString:@":"]) {
      NSString *base = directory.isAbsolutePath ? directory
        : [cwd stringByAppendingPathComponent:directory];
      [candidates addObject:[base stringByAppendingPathComponent:@"dnr"]];
    }
  }
  // Finder/LaunchServices usually supplies only /usr/bin:/bin:/usr/sbin:/sbin.
  // These stable locations survive Homebrew version upgrades. Never invoke a
  // login shell or source user shell configuration to discover a runtime.
  [candidates addObjectsFromArray:@[@"/opt/homebrew/bin/dnr",
                                    @"/opt/homebrew/opt/dnr/bin/dnr",
                                    @"/usr/local/bin/dnr"]];
  if (home.length) [candidates addObject:[home stringByAppendingPathComponent:@".local/bin/dnr"]];
  return candidates.array;
}

static NSString *findRuntime(NSArray<NSString *> *candidates) {
  NSFileManager *files = NSFileManager.defaultManager;
  for (NSString *candidate in candidates) {
    BOOL directory = NO;
    if ([files fileExistsAtPath:candidate isDirectory:&directory] && !directory &&
        [files isExecutableFileAtPath:candidate]) return candidate;
  }
  return nil;
}

int main(int argc, char **argv) {
  @autoreleasepool {
    NSBundle *bundle = NSBundle.mainBundle;
    id configured = [bundle objectForInfoDictionaryKey:@"DNRRuntimePath"];
    NSString *package = [bundle pathForResource:@"application" ofType:@"dnp"];
    if (!package || (configured && ![configured isKindOfClass:NSString.class]))
      return fail(@"应用资源不完整，请重新安装应用。");
    NSArray<NSString *> *candidates = runtimeCandidates(configured,
      NSProcessInfo.processInfo.environment[@"PATH"], NSHomeDirectory(),
      NSFileManager.defaultManager.currentDirectoryPath);
    NSString *runtime = findRuntime(candidates);
    if (!runtime)
      return fail([NSString stringWithFormat:@"找不到共享 dnr 运行时。请安装 dnr，或检查应用的运行时配置。\n已查找：\n%@",
                   [candidates componentsJoinedByString:@"\n"]]);
    // Preserve argv boundaries, inherited descriptors, environment and cwd.
    char **args = calloc((size_t)argc + 2, sizeof(char *));
    if (!args) return fail(@"无法分配启动参数。");
    args[0] = (char *)runtime.fileSystemRepresentation;
    args[1] = (char *)package.fileSystemRepresentation;
    for (int i = 1; i < argc; i++) args[i + 1] = argv[i];
    int status = runRuntime(args);
    int error = errno;
    free(args);
    if (status >= 0) return status;
    return fail([NSString stringWithFormat:@"无法启动 dnr：%s", strerror(error)]);
  }
}
