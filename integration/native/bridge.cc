#include <cstring>
#include <iostream>
#ifdef DNR_LAZY_GUI
#include "gui_loader.h"
#endif

extern "C" bool dnr_runtime_is_initialized();
namespace {
enum class Backend { Auto, Cef, WebView };
Backend selected = Backend::Auto;
}

#ifdef DNR_WEBVIEW
int dnr_webview_main(int argc, char** argv);
#endif
#ifdef DNR_SYSTEM_CEF
#include "include/cef_api_hash.h"
#include "include/cef_version.h"
#include <unistd.h>
int dnr_cef_main(int argc, char** argv);
static int check_cef() {
#ifdef DNR_LAZY_GUI
  if (!dnr_load_gui(DnrGuiCef)) return 78;
#endif
  const char* hash = cef_api_hash(CEF_API_VERSION, 0);
  if (!hash || std::strcmp(hash, CEF_API_HASH_PLATFORM)) {
    std::cerr << "dnr: system CEF API/hash mismatch; rebuild against the installed CEF package\n";
    return 78;
  }
  return 0;
}

static int start_cef(int argc, char** argv) {
  if (int result = check_cef()) return result;
  // Missing resources can make Chromium abort instead of returning an error.
  for (const char* path : {"/usr/lib/cef/icudtl.dat", "/usr/lib/cef/resources.pak",
                           "/usr/lib/cef/v8_context_snapshot.bin", "/usr/lib/cef/locales"}) {
    if (access(path, R_OK) != 0) {
      std::cerr << "dnr: missing or unreadable system CEF resource: " << path << '\n';
      return 78;
    }
  }
  return dnr_cef_main(argc, argv);
}
#endif

extern "C" int dnr_native_preflight(int argc, char** argv, const char* backend) {
  if (!std::strcmp(backend, "system-cef")) selected = Backend::Cef;
  else if (!std::strcmp(backend, "webview")) selected = Backend::WebView;
  else if (!std::strcmp(backend, "auto")) selected = Backend::Auto;
  else return 2;

  if (argc == 2 && !std::strcmp(argv[1], "--check-system-cef")) {
#ifdef DNR_SYSTEM_CEF
    if (int result = check_cef()) return result;
    std::cout << "System CEF ABI OK: API " << CEF_API_VERSION << ", headers " << CEF_VERSION << '\n';
    return 0;
#else
    std::cerr << "dnr: this build does not include system CEF\n";
    return 2;
#endif
  }
#ifdef DNR_SYSTEM_CEF
  // Chromium puts the process type first. Do not interpret application arguments
  // (after a script/package path) as Chromium switches.
  if (argc > 1 && !std::strncmp(argv[1], "--type=", 7)) {
    if (int result = check_cef()) return result;
    return dnr_cef_main(argc, argv);
  }
#else
  if (selected == Backend::Cef) {
    std::cerr << "dnr: this build does not include system CEF\n";
    return 2;
  }
#endif
#ifndef DNR_WEBVIEW
  if (selected == Backend::WebView) {
    std::cerr << "dnr: this build does not include WebView\n";
    return 2;
  }
#endif
  return -1;
}

extern "C" int dnr_native_main(int argc, char** argv) {
#ifdef DNR_SYSTEM_CEF
  if (selected != Backend::WebView) {
    int result = start_cef(argc, argv);
#ifdef DNR_WEBVIEW
    // Once the runtime has received a backend API, it may already have live
    // windows/tasks. Never replay the application or replace that API.
    if (selected == Backend::Cef || dnr_runtime_is_initialized()) return result;
    std::cerr << "dnr: system CEF failed to initialize (" << result
              << "); falling back to WebView\n";
#else
    return result;
#endif
  }
#endif
#ifdef DNR_WEBVIEW
#ifdef DNR_LAZY_GUI
  if (!dnr_load_gui(DnrGuiWebView)) return 78;
#endif
  return dnr_webview_main(argc, argv);
#else
  return 2;
#endif
}
