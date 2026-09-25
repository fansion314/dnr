#pragma once

// Only the main thread initializes a backend. Imports and dlopen handles remain
// valid until process exit, including after a failed CEF initialization.
enum DnrGuiBackend : unsigned { DnrGuiCef = 1, DnrGuiWebView = 2 };
bool dnr_load_gui(unsigned backend);
