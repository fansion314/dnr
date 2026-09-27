#define main launcher_main
#include "../templates/macos-launcher.m"
#undef main
#include <assert.h>
#include <sys/stat.h>

int main(int argc, char **argv) {
  @autoreleasepool {
    assert(argc == 2);
    NSString *root = [NSString stringWithUTF8String:argv[1]];
    NSArray *paths = runtimeCandidates(@"dnr", @"first bin::relative", root, root);
    assert(([paths[0] isEqualToString:[root stringByAppendingPathComponent:@"first bin/dnr"]]));
    assert(([paths[1] isEqualToString:[root stringByAppendingPathComponent:@"dnr"]]));
    assert(([paths[2] isEqualToString:[root stringByAppendingPathComponent:@"relative/dnr"]]));
    NSArray *finder = runtimeCandidates(nil, @"/usr/bin:/bin", root, root);
    assert(([finder containsObject:@"/opt/homebrew/bin/dnr"]));
    assert(([finder containsObject:@"/opt/homebrew/opt/dnr/bin/dnr"]));
    assert(([finder.lastObject isEqualToString:[root stringByAppendingPathComponent:@".local/bin/dnr"]]));
    NSString *missing = [root stringByAppendingPathComponent:@"missing"];
    assert(runtimeCandidates(missing, @"/opt/homebrew/bin", root, root).count == 1);
    assert(findRuntime(runtimeCandidates(missing, @"/opt/homebrew/bin", root, root)) == nil);
    assert(runtimeCandidates(@"bad/relative-pin", @"/opt/homebrew/bin", root, root).count == 0);
    NSString *first = [root stringByAppendingPathComponent:@"first"];
    NSString *second = [root stringByAppendingPathComponent:@"second"];
    [@"#!/bin/sh\nexit 0\n" writeToFile:first atomically:YES encoding:NSUTF8StringEncoding error:NULL];
    [@"#!/bin/sh\nexit 0\n" writeToFile:second atomically:YES encoding:NSUTF8StringEncoding error:NULL];
    assert(chmod(first.fileSystemRepresentation, 0644) == 0);
    assert(chmod(second.fileSystemRepresentation, 0755) == 0);
    assert(([findRuntime(@[missing, root, first, second]) isEqualToString:second]));
    assert(chmod(first.fileSystemRepresentation, 0755) == 0);
    assert(([findRuntime(@[first, second]) isEqualToString:first]));
    puts("DNR_LAUNCHER_PATH_OK");
  }
}
