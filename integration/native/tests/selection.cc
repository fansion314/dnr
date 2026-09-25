// Exercise dispatch/failure handling without opening a display or starting V8.
#include "include/cef_api_hash.h"
#include <cassert>
#include <cstring>
#include <string>
#include <unistd.h>

extern "C" int dnr_native_preflight(int, char**, const char*);
extern "C" int dnr_native_main(int, char**);
static bool initialized = false;
static bool matching_hash = true;
static bool resources_present = true;
static int cef_calls = 0, webview_calls = 0, hash_calls = 0;
static int cef_status = 0;
static bool cef_ready = false;
extern "C" bool dnr_runtime_is_initialized() { return initialized; }
extern "C" const char* cef_api_hash(int, int) {
  ++hash_calls;
  return matching_hash ? CEF_API_HASH_PLATFORM : "incompatible";
}
// Interpose resource checks so tests don't require an installed CEF runtime.
extern "C" int access(const char*, int) noexcept { return resources_present ? 0 : -1; }
int dnr_cef_main(int, char**) {
  ++cef_calls;
  initialized = cef_ready;
  return cef_status;
}
int dnr_webview_main(int, char**) { ++webview_calls; initialized = true; return 0; }
int main(int argc, char** argv) {
  assert(argc == 2);
  std::string test = argv[1];
  char name[] = "dnr", script[] = "app.ts", type[] = "--type=renderer";
  char* app_args[] = {name, script, type, nullptr};
  const char* selection = test == "explicit-cef" ? "system-cef" :
                          test == "explicit-webview" ? "webview" : "auto";
  if (test == "helper") {
    char* helper_args[] = {name, type, nullptr};
    cef_status = 7;
    assert(dnr_native_preflight(2, helper_args, "auto") == 7);
    assert(cef_calls == 1 && webview_calls == 0);
    return 0;
  }
  assert(dnr_native_preflight(3, app_args, selection) == -1);
  assert(hash_calls == 0 && cef_calls == 0 && webview_calls == 0);
  if (test == "headless") return 0;
  if (test == "resource-fallback") resources_present = false;
  if (test == "abi-fallback" || test == "explicit-webview") matching_hash = false;
  if (test == "init-fallback" || test == "explicit-cef" || test == "after-ready") cef_status = 9;
  if (test == "success" || test == "after-ready") cef_ready = true;
  int result = dnr_native_main(1, app_args);
  if (test == "explicit-cef" || test == "after-ready") {
    assert(result == 9 && cef_calls == 1 && webview_calls == 0);
  } else if (test == "success") {
    assert(result == 0 && cef_calls == 1 && webview_calls == 0);
  } else {
    assert(result == 0 && webview_calls == 1);
    if (test == "abi-fallback" || test == "resource-fallback" || test == "explicit-webview") assert(cef_calls == 0);
    if (test == "explicit-webview") assert(hash_calls == 0);
  }
}
