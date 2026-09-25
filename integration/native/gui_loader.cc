#include "gui_loader.h"
#include <cstdio>
#include <cstdlib>
#include <dlfcn.h>
#include <mutex>
#include <vector>

struct DnrGuiLibrary { const char* name; void* handle; };
struct DnrGuiImport {
  const char* name;
  unsigned library;
  unsigned backends;
  void** slot;
};
#include "gui_imports.inc"

extern "C" [[noreturn]] void dnr_unloaded_gui_import() {
  std::fputs("dnr: GUI function called before backend library initialization\n", stderr);
  std::abort();
}

bool dnr_load_gui(unsigned backend) {
  static std::mutex mutex;
  static unsigned ready = 0;
  const std::lock_guard<std::mutex> lock(mutex);
  if ((ready & backend) == backend) return true;
  // Resolve the complete backend before publishing any of its slots. A missing
  // symbol must return through the normal selection/fallback path.
  std::vector<void*> addresses;
  for (const auto& item : dnr_gui_imports) {
    if (!(item.backends & backend)) continue;
    auto& library = dnr_gui_libraries[item.library];
    if (!library.handle) {
      library.handle = dlopen(library.name, RTLD_NOW | RTLD_LOCAL);
      if (!library.handle) {
        std::fprintf(stderr, "dnr: cannot load GUI library %s: %s\n", library.name, dlerror());
        return false;
      }
    }
    dlerror();
    void* address = dlsym(library.handle, item.name);
    const char* error = dlerror();
    if (error || !address) {
      std::fprintf(stderr, "dnr: cannot resolve GUI function %s in %s: %s\n",
                   item.name, library.name, error ? error : "null address");
      return false;
    }
    addresses.push_back(address);
  }
  size_t index = 0;
  for (const auto& item : dnr_gui_imports) {
    if (!(item.backends & backend)) continue;
    // Common imports may already be in use after a CEF initialization failure.
    // Their provider handle is retained, so never rewrite a published slot.
    if (*item.slot == reinterpret_cast<void*>(&dnr_unloaded_gui_import)) {
      *item.slot = addresses[index];
    }
    ++index;
  }
  ready |= backend;
  return true;
}
