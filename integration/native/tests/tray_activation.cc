// Compile with the patched backend-common src/include and capi/include paths,
// pkg-config gtk+-3.0, and -ldl. Run with gui-run timeout 15s <test-binary>.
// Include the implementation so the test can emit the real GObject signal.
#include "tray_linux.cc"
#include <cassert>

namespace laufey_common {
// Menu conversion is independent of activation and is not exercised here.
GtkWidget* BuildGtkMenuFromValue(laufey_value_t*, const laufey_backend_api_t*,
                                uint32_t, laufey_menu_click_fn, void*, bool) {
  return nullptr;
}
}

static void Drain() {
  while (g_main_context_iteration(nullptr, false)) {}
}

int main(int argc, char** argv) {
  gtk_init(&argc, &argv);
  using namespace laufey_common;
  if (argc == 2) {
    auto id = CreateTrayIconLinux();
    assert(id);
    gchar* bytes = nullptr;
    gsize size = 0;
    assert(g_file_get_contents(argv[1], &bytes, &size, nullptr));
    SetTrayIconLinux(id, bytes, size);
    g_free(bytes);
    auto* window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), "DNR tray activation fixture");
    gtk_window_set_default_size(GTK_WINDOW(window), 440, 180);
    gtk_container_add(GTK_CONTAINER(window), gtk_label_new("Close, then left-click the tray icon to restore."));
    g_signal_connect(window, "delete-event", G_CALLBACK(+[](GtkWidget* win, GdkEvent*, gpointer) -> gboolean {
      gtk_widget_hide(win);
      puts("TRAY_WINDOW_HIDDEN"); fflush(stdout);
      return TRUE;
    }), nullptr);
    SetTrayClickHandlerLinux(id, +[](void* data, uint32_t) {
      gtk_widget_show_all(GTK_WIDGET(data));
      gtk_window_present(GTK_WINDOW(data));
      puts("TRAY_LEFT_CLICK_RESTORED"); fflush(stdout);
    }, window);
    Drain();
    auto* menu = gtk_menu_new();
    auto* quit = gtk_menu_item_new_with_label("Quit tray fixture");
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), quit);
    g_signal_connect(quit, "activate", G_CALLBACK(+[](GtkMenuItem*, gpointer) {
      puts("TRAY_MENU_QUIT"); fflush(stdout); gtk_main_quit();
    }), nullptr);
    gtk_widget_show_all(menu);
    GetAppIndicatorApi()->set_menu(LinuxTrayMap().at(id).indicator, GTK_MENU(menu));
    gtk_widget_show_all(window);
    puts("TRAY_INTERACTIVE_READY"); fflush(stdout);
    gtk_main();
    DestroyTrayIconLinux(id);
    Drain();
    return 0;
  }
  auto first = CreateTrayIconLinux();
  auto second = CreateTrayIconLinux();
  assert(first && second && first != second);
  Drain();
  auto* indicator = LinuxTrayMap().at(first).indicator;
  if (!g_signal_lookup("activate", G_OBJECT_TYPE(indicator))) return 77;
  struct Calls { uint32_t id; int count; } a{first, 0}, b{second, 0};
  auto callback = +[](void* data, uint32_t id) {
    auto* calls = static_cast<Calls*>(data);
    assert(calls->id == id);
    // Reentrant map access proves callbacks do not run under the map mutex.
    std::lock_guard<std::mutex> lock(LinuxTrayMutex());
    assert(LinuxTrayMap().count(id));
    calls->count++;
  };
  SetTrayClickHandlerLinux(first, callback, &a);
  SetTrayClickHandlerLinux(second, callback, &b);
  Drain();
  g_signal_emit_by_name(indicator, "activate", 10, 20);
  assert(a.count == 1 && b.count == 0);
  SetTrayClickHandlerLinux(first, callback, &a);
  Drain();
  g_signal_emit_by_name(indicator, "activate", 10, 20);
  assert(a.count == 2); // replacement must not accumulate signal handlers
  SetTrayClickHandlerLinux(first, nullptr, nullptr);
  Drain();
  g_signal_emit_by_name(indicator, "activate", 10, 20);
  assert(a.count == 2);
  g_signal_emit_by_name(LinuxTrayMap().at(second).indicator, "activate", 1, 2);
  assert(b.count == 1);
  DestroyTrayIconLinux(first);
  DestroyTrayIconLinux(second);
  Drain();
  assert(LinuxTrayMap().empty());
  puts("TRAY_ACTIVATION_OK");
}
