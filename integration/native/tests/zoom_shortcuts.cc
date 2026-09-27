#include "desktop_zoom.h"
#include <cassert>

static bool enabled = true;
static int calls = 0;
static int last = 0;
static bool shortcut(int action) {
  if (!enabled) return false;
  if (action) { ++calls; last = action; }
  return true;
}

int main() {
  DnrSetZoomShortcutHandler(nullptr, shortcut);
#ifdef __APPLE__
  const auto mod = LAUFEY_MOD_META;
#else
  const auto mod = LAUFEY_MOD_CONTROL;
#endif
  assert(!DnrZoomKey(0, true, "=", "Equal", mod, false));
  assert(!DnrZoomKey(1, true, "=", "Equal", 0, false));
  assert(DnrZoomKey(1, true, "=", "Equal", mod, false));
  assert(calls == 1 && last == 1);
  assert(DnrZoomKey(1, true, "=", "Equal", mod, true));
  assert(DnrZoomKey(1, true, "=", "Equal", mod, false));
  assert(calls == 1);
  assert(DnrZoomKey(1, false, "=", "Equal", 0, false));
  assert(DnrZoomKey(1, true, "+", "Equal", mod | LAUFEY_MOD_SHIFT, false));
  assert(calls == 2);
  DnrClearZoomKeys(1);
  assert(DnrZoomKey(1, true, "-", "NumpadSubtract", mod, false));
  assert(last == -1);
  assert(DnrZoomKey(1, true, "0", "Digit0", mod, false));
  assert(last == 2);
  DnrClearZoomKeys(1);
  assert(DnrZoomKey(1, true, "Unidentified", "Equal", mod, false));
  assert(last == 1);
  assert(DnrZoomKey(1, true, "=", "Equal", mod, false, false));
  const int cef_calls = calls;
  assert(DnrZoomKey(1, true, "=", "Equal", mod, true, false));
  assert(calls == cef_calls);
  assert(DnrZoomKey(1, true, "=", "Equal", mod, false, false));
  assert(calls == cef_calls + 1);
  enabled = false;
  assert(!DnrZoomKey(1, true, "+", "NumpadAdd", mod, false));
  enabled = true;
  dnr_zoom_exempt.insert(2);
  assert(!DnrZoomKey(2, true, "+", "NumpadAdd", mod, false));
  dnr_content_zoom = 1.5;
  assert(DnrWindowZoom(1) == 1.5 && DnrWindowZoom(2) == 1.0);
}
