#pragma once

#include <atomic>
#include <cmath>
#include <set>
#include <string>
#include <utility>
#include "laufey.h"

// Only one desktop backend is active per process, including in dual builds.
// Publication is synchronous; native UI work always reads the latest value.
inline std::atomic<double> dnr_content_zoom{1.0};
inline std::atomic<bool (*)(int)> dnr_zoom_shortcut{nullptr};
inline std::set<std::pair<uint32_t, std::string>> dnr_zoom_pressed;
inline std::set<uint32_t> dnr_zoom_exempt;
inline double DnrWindowZoom(uint32_t id) {
  return dnr_zoom_exempt.contains(id) ? 1.0 : dnr_content_zoom.load();
}

inline void DnrSetZoomShortcutHandler(void*, bool (*handler)(int)) {
  dnr_zoom_shortcut.store(handler);
}

inline double DnrCefZoomLevel(uint32_t id) {
  return std::log(DnrWindowZoom(id)) / std::log(1.2);
}

inline int DnrZoomAction(const std::string& key, const std::string& code,
                        uint32_t modifiers) {
#ifdef __APPLE__
  constexpr uint32_t primary = LAUFEY_MOD_META;
#else
  constexpr uint32_t primary = LAUFEY_MOD_CONTROL;
#endif
  if ((modifiers & ~LAUFEY_MOD_SHIFT) != primary) return 0;
  // CEF raw key events may omit a printable character. Fall back to the
  // normalized physical key only in that case, not for other printable keys.
  const bool unidentified = key.empty() || key == "Unidentified";
  int action = 0;
  if (key == "+" || key == "=" || code == "NumpadAdd" || (unidentified && code == "Equal")) action = 1;
  else if (key == "-" || code == "NumpadSubtract" || (unidentified && code == "Minus")) action = -1;
  else if (key == "0" || code == "Numpad0") action = 2;
  return action;
}

// Runs on the native UI thread before WebKit/Chromium receives the event.
inline bool DnrZoomKey(uint32_t window_id, bool pressed,
                       const std::string& key, const std::string& code,
                       uint32_t modifiers, bool repeat, bool track_pressed = true) {
  if (!window_id || dnr_zoom_exempt.contains(window_id)) return false;
  const auto identity = std::make_pair(window_id, code);
  if (!pressed) return track_pressed && dnr_zoom_pressed.erase(identity) != 0;
  auto handler = dnr_zoom_shortcut.load();
  if (!handler || !handler(0)) return false;
  const int action = DnrZoomAction(key, code, modifiers);
  if (!action) return false;
  if (repeat || (track_pressed && !dnr_zoom_pressed.insert(identity).second)) return true;
  return handler(action);
}

inline void DnrClearZoomKeys(uint32_t window_id) {
  for (auto it = dnr_zoom_pressed.begin(); it != dnr_zoom_pressed.end();) {
    if (it->first == window_id) it = dnr_zoom_pressed.erase(it);
    else ++it;
  }
}
