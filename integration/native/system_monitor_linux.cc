#include "system_monitor.h"
#include <gtk/gtk.h>
#include <gdk/gdkx.h>
#include <gio/gio.h>
#include <X11/extensions/scrnsaver.h>
#include <dlfcn.h>
#include <unistd.h>
#include <cstring>
#include <vector>

namespace {
laufey_system_event_fn handler = nullptr;
void* handler_data = nullptr;
struct Subscription { GDBusConnection* connection; guint id; };
std::vector<Subscription> subscriptions;
void Emit(const char* event) { if (handler) handler(handler_data, event); }
bool HasOwner(GDBusConnection* connection, const char* name) {
  GVariant* reply = g_dbus_connection_call_sync(connection, "org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "NameHasOwner", g_variant_new("(s)", name), G_VARIANT_TYPE("(b)"),
      G_DBUS_CALL_FLAGS_NONE, 1000, nullptr, nullptr);
  if (!reply) return false;
  gboolean owned = FALSE; g_variant_get(reply, "(b)", &owned); g_variant_unref(reply); return owned;
}
void Screensaver(GDBusConnection*, const gchar*, const gchar*, const gchar*, const gchar*, GVariant* value, gpointer) {
  if (!g_variant_is_of_type(value, G_VARIANT_TYPE("(b)"))) return;
  gboolean active = FALSE; g_variant_get(value, "(b)", &active); if (active) Emit("lock");
}
void Sleep(GDBusConnection*, const gchar*, const gchar*, const gchar*, const gchar*, GVariant* value, gpointer) {
  if (!g_variant_is_of_type(value, G_VARIANT_TYPE("(b)"))) return;
  gboolean preparing = FALSE; g_variant_get(value, "(b)", &preparing); Emit(preparing ? "suspend" : "resume");
}
void Lock(GDBusConnection*, const gchar*, const gchar*, const gchar*, const gchar*, GVariant*, gpointer) { Emit("lock"); }
void Subscribe(GDBusConnection* connection, const char* sender, const char* interface,
               const char* member, const char* path, GDBusSignalCallback callback) {
  guint id = g_dbus_connection_signal_subscribe(connection, sender, interface, member, path, nullptr,
      G_DBUS_SIGNAL_FLAGS_NONE, callback, nullptr, nullptr);
  subscriptions.push_back({G_DBUS_CONNECTION(g_object_ref(connection)), id});
}
}

double DnrSystemIdleSeconds() {
  // XWayland only observes some applications; never advertise it as session idle.
  const char* session = g_getenv("XDG_SESSION_TYPE");
  if (session && std::strcmp(session, "wayland") == 0) return -1;
  GdkDisplay* display = gdk_display_get_default();
  if (!display || !GDK_IS_X11_DISPLAY(display)) return -1;
  static void* library = dlopen("libXss.so.1", RTLD_NOW | RTLD_LOCAL);
  static auto query = library ? reinterpret_cast<decltype(&XScreenSaverQueryInfo)>(dlsym(library, "XScreenSaverQueryInfo")) : nullptr;
  if (!query) return -1;
  Display* xdisplay = gdk_x11_display_get_xdisplay(display);
  XScreenSaverInfo info{};
  if (!query(xdisplay, DefaultRootWindow(xdisplay), &info)) return -1;
  return static_cast<double>(info.idle) / 1000.0;
}

uint32_t DnrSetSystemMonitor(laufey_system_event_fn callback, void* data) {
  handler = nullptr;
  for (const auto& subscription : subscriptions) {
    g_dbus_connection_signal_unsubscribe(subscription.connection, subscription.id);
    g_object_unref(subscription.connection);
  }
  subscriptions.clear(); handler_data = data; handler = callback;
  if (!callback) return 0;
  uint32_t capabilities = DnrSystemIdleSeconds() >= 0 ? 8 : 0;
  GDBusConnection* session = g_bus_get_sync(G_BUS_TYPE_SESSION, nullptr, nullptr);
  if (session) {
    const char* services[][2] = {{"org.freedesktop.ScreenSaver","org.freedesktop.ScreenSaver"},
      {"org.gnome.ScreenSaver","org.gnome.ScreenSaver"}, {"org.kde.screensaver","org.freedesktop.ScreenSaver"}};
    for (const auto& service : services) if (HasOwner(session, service[0])) {
      Subscribe(session, service[0], service[1], "ActiveChanged", nullptr, Screensaver); capabilities |= 1;
    }
    g_object_unref(session);
  }
  GDBusConnection* system = g_bus_get_sync(G_BUS_TYPE_SYSTEM, nullptr, nullptr);
  if (system) {
    if (HasOwner(system, "org.freedesktop.login1")) {
      Subscribe(system, "org.freedesktop.login1", "org.freedesktop.login1.Manager", "PrepareForSleep", "/org/freedesktop/login1", Sleep);
      capabilities |= 6;
      GVariant* reply = g_dbus_connection_call_sync(system, "org.freedesktop.login1", "/org/freedesktop/login1",
          "org.freedesktop.login1.Manager", "GetSessionByPID", g_variant_new("(u)", static_cast<guint32>(getpid())),
          G_VARIANT_TYPE("(o)"), G_DBUS_CALL_FLAGS_NONE, 1000, nullptr, nullptr);
      if (reply) {
        const gchar* path = nullptr; g_variant_get(reply, "(&o)", &path);
        Subscribe(system, "org.freedesktop.login1", "org.freedesktop.login1.Session", "Lock", path, Lock);
        capabilities |= 1; g_variant_unref(reply);
      }
    }
    g_object_unref(system);
  }
  return capabilities;
}
