#include <cstring>
#include <iostream>
int dnr_backend_main_impl(int argc, char** argv);

#ifdef DNR_SYSTEM_CEF
#include "include/cef_api_hash.h"
#include "include/cef_version.h"
static int check_cef() {
  const char* hash = cef_api_hash(CEF_API_VERSION, 0);
  if (!hash || std::strcmp(hash, CEF_API_HASH_PLATFORM)) {
    std::cerr << "dnr: system CEF API/hash mismatch; rebuild against the installed CEF package\n";
    return 78;
  }
  return 0;
}
#endif

extern "C" int dnr_native_preflight(int argc, char** argv) {
#ifdef DNR_SYSTEM_CEF
  if (int result = check_cef()) return result;
  if (argc == 2 && !std::strcmp(argv[1], "--check-system-cef")) {
    std::cout << "System CEF ABI OK: API " << CEF_API_VERSION << ", headers " << CEF_VERSION << '\n';
    return 0;
  }
  for (int i = 1; i < argc; ++i) {
    if (!std::strncmp(argv[i], "--type=", 7)) return dnr_backend_main_impl(argc, argv);
  }
#else
  if (argc == 2 && !std::strcmp(argv[1], "--check-system-cef")) {
    std::cerr << "dnr: this build uses the system WebView, not CEF\n";
    return 2;
  }
#endif
  return -1;
}

extern "C" int dnr_native_main(int argc, char** argv) {
  return dnr_backend_main_impl(argc, argv);
}
