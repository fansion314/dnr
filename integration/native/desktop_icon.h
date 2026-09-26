#pragma once
#include <gtk/gtk.h>
#include <cstdint>
#include <cstdlib>
#include <iostream>

extern "C" const unsigned char* dnr_window_icon(uint32_t*, uint32_t*);

// Caller owns a reference. The metadata buffer is immutable for the process lifetime.
// Legacy launchers/scripts can still supply a real file through LAUFEY_APP_ICON.
static GdkPixbuf* dnr_load_window_icon() {
  uint32_t width = 0, height = 0;
  if (const auto* rgba = dnr_window_icon(&width, &height)) {
    GBytes* bytes = g_bytes_new_static(rgba, width * height * 4);
    auto* pixbuf = gdk_pixbuf_new_from_bytes(bytes, GDK_COLORSPACE_RGB, TRUE,
                                           8, width, height, width * 4);
    g_bytes_unref(bytes);
    return pixbuf;
  }
  if (const char* path = std::getenv("LAUFEY_APP_ICON"); path && *path) {
    GError* error = nullptr;
    auto* pixbuf = gdk_pixbuf_new_from_file_at_scale(path, 128, 128, TRUE, &error);
    if (error) {
      std::cerr << "dnr: could not load window icon: " << error->message << '\n';
      g_error_free(error);
    }
    return pixbuf;
  }
  return nullptr;
}
