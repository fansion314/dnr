#!/usr/bin/env python3
"""Real ELF integration tests for the generated import table (Linux x86_64)."""
import importlib.util
import json
import os
import pathlib
import platform
import subprocess
import tempfile
import unittest

NATIVE = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("generate_gui_imports", NATIVE / "generate_gui_imports.py")
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64", "requires Linux x86_64 ELF")
class GuiImports(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="dnr-gui-imports-")
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.library = self.root / "libdnr-test-gui.so"
        self.provider = self.write("provider.c", """
#include <stdarg.h>
long gui_integer(long a,long b,long c,long d,long e,long f,long g,long h) {
  return a+2*b+3*c+4*d+5*e+6*f+7*g+8*h;
}
double gui_float(double a,double b,double c,double d,double e,double f,double g,double h,double i) {
  return a+b+c+d+e+f+g+h+i;
}
double gui_variadic(int count, ...) {
  va_list args; va_start(args, count); double sum = 0;
  for (int i = 0; i < count; ++i) sum += va_arg(args, double);
  va_end(args); return sum;
}
int gui_data = 5;
""")
        self.build_provider()

    def command(self, *args, **kwargs):
        return subprocess.run([str(arg) for arg in args], check=True, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)

    def write(self, name, contents):
        path = self.root / name
        path.write_text(contents)
        return path

    def build_provider(self):
        self.command("cc", "-shared", "-fPIC", self.provider,
                     "-Wl,-soname,libdnr-test-gui.so", "-o", self.library)

    def generate(self, source, group="webview"):
        source = self.write("caller.c", source)
        obj = self.root / "caller.o"
        self.command("cc", "-fPIC", "-c", source, "-o", obj)
        manifest = self.write("manifest", f"{group}|{obj}\nlibrary|{self.library}\n")
        generator.generate(manifest, self.root, "readelf")
        return obj

    def executable(self):
        obj = self.generate("""
extern long gui_integer(long,long,long,long,long,long,long,long);
extern double gui_float(double,double,double,double,double,double,double,double,double);
extern double gui_variadic(int,...);
int exercise(void) {
  if (gui_integer(1,2,3,4,5,6,7,8) != 204) return 1;
  if (gui_float(1,2,3,4,5,6,7,8,9) != 45) return 2;
  if (gui_variadic(10,1.,2.,3.,4.,5.,6.,7.,8.,9.,10.) != 55) return 3;
  return 0;
}
""")
        driver = self.write("driver.cc", """
#include "gui_loader.h"
extern "C" int exercise();
int main(int argc, char**) {
  if (argc == 1) return 0;
  if (!dnr_load_gui(DnrGuiWebView)) return 78;
  if (!dnr_load_gui(DnrGuiWebView)) return 79;
  return exercise();
}
""")
        executable = self.root / "driver"
        flags = (self.root / "gui-wrap-flags").read_text().splitlines()
        self.command("c++", "-std=c++20", "-Wall", "-Wextra", "-Werror", "-fPIE", "-pie",
                     "-I", NATIVE, "-I", self.root, driver, obj,
                     NATIVE / "gui_loader.cc", self.root / "gui_trampolines.S",
                     *flags, "-ldl", "-o", executable)
        return executable

    def run_driver(self, executable, gui=False):
        return subprocess.run([str(executable)] + (["gui"] if gui else []), text=True,
                              capture_output=True, env={**os.environ, "LD_LIBRARY_PATH": str(self.root)})

    def test_pie_integer_float_stack_and_varargs_abi(self):
        executable = self.executable()
        result = self.run_driver(executable, gui=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        dynamic = self.command("readelf", "-d", executable).stdout
        self.assertNotIn("libdnr-test-gui", dynamic)
        undefined = self.command("readelf", "--dyn-syms", "--wide", executable).stdout
        self.assertNotIn("gui_integer", undefined)

    def test_cli_without_gui_library_and_graceful_load_failure(self):
        executable = self.executable()
        self.library.unlink()
        self.assertEqual(self.run_driver(executable).returncode, 0)
        result = self.run_driver(executable, gui=True)
        self.assertEqual(result.returncode, 78)
        self.assertIn("cannot load GUI library libdnr-test-gui.so", result.stderr)

    def test_missing_export_fails_before_calling_backend(self):
        executable = self.executable()
        self.provider.write_text("void gui_integer(void) {}\n")
        self.build_provider()
        result = self.run_driver(executable, gui=True)
        self.assertEqual(result.returncode, 78)
        self.assertIn("cannot resolve GUI function", result.stderr)

    def test_data_import_rejected(self):
        with self.assertRaisesRegex(ValueError, "non-function GUI import gui_data"):
            self.generate("extern int gui_data; int use(void) { return gui_data; }")

    def test_weak_import_rejected(self):
        with self.assertRaisesRegex(ValueError, "weak GUI import"):
            self.generate("extern long gui_integer(void) __attribute__((weak)); long use(void) { return gui_integer(); }")

    def test_explicit_symbol_version_rejected(self):
        with self.assertRaisesRegex(ValueError, "versioned/mangled GUI import"):
            self.generate('extern long versioned(void); __asm__(".symver versioned,gui_integer@TEST_1"); long use(void) { return versioned(); }')

    def test_backend_membership(self):
        self.generate("extern long gui_integer(void); long use(void) { return gui_integer(); }", "cef")
        imports = json.loads((self.root / "gui-imports.json").read_text())
        self.assertEqual(imports, [{"symbol": "gui_integer", "library": "libdnr-test-gui.so", "backends": 1}])

    def test_cpp_runtime_export_does_not_add_webview_dependency_to_cef(self):
        # JavaScriptCore exports some libstdc++ template instantiations. Model
        # that with a real DSO exporting std::string methods, then compile a CEF
        # caller that needs those methods as well as its ordinary C GUI API.
        jsc_source = self.write("jsc.cc", """
#include <string>
template class std::basic_string<char>;
""")
        jsc = self.root / "libjavascriptcoregtk-test.so.0"
        self.command("c++", "-std=c++20", "-fPIC", "-shared", jsc_source,
                     "-Wl,-soname,libjavascriptcoregtk-test.so.0", "-o", jsc)
        exports = {name.split("@")[0]
                   for name, kind, _, _, section in generator.symbols("readelf", jsc, True)
                   if section != "UND" and kind == "FUNC" and "basic_string" in name}
        stdcpp = self.command("c++", "-print-file-name=libstdc++.so").stdout.strip()
        runtime_exports = {name.split("@")[0]
                           for name, kind, _, _, section in generator.symbols("readelf", stdcpp, True)
                           if section != "UND" and kind == "FUNC"}
        shared = sorted(exports & runtime_exports)
        self.assertTrue(shared, "fixture must export actual libstdc++ std::string functions")
        # Keep a relocation to an actual shared function even if this GCC
        # version inlines all calls produced by a simple std::string expression.
        caller = self.write("cef.cc", f'''
#include <cstring>
#include <dlfcn.h>
extern "C" void runtime_symbol() asm("{shared[0]}");
using RuntimeFunction = void (*)();
RuntimeFunction volatile runtime_reference = &runtime_symbol;
extern "C" long gui_integer(long,long,long,long,long,long,long,long);
extern "C" int exercise() {{
  Dl_info info{{}};
  if (!dladdr(reinterpret_cast<void*>(runtime_reference), &info)) return 1;
  if (!std::strstr(info.dli_fname, "libstdc++")) return 2;
  return gui_integer(1,2,3,4,5,6,7,8) != 204;
}}
''')
        obj = self.root / "cef.o"
        self.command("c++", "-std=c++20", "-O0", "-fPIC",
                     "-c", caller, "-o", obj)
        refs = {name for name, _, _, _, section in generator.symbols("readelf", obj)
                if section == "UND" and name.startswith("_Z")}
        self.assertTrue(refs & exports, "fixture must actually reproduce overlapping C++ exports")
        manifest = self.write("manifest", f"cef|{obj}\nlibrary|{self.library}\nlibrary|{jsc}\n")
        generator.generate(manifest, self.root, "readelf")
        imports = json.loads((self.root / "gui-imports.json").read_text())
        self.assertEqual(imports, [{"symbol": "gui_integer", "library": "libdnr-test-gui.so", "backends": 1}])
        driver = self.write("driver.cc", """
#include "gui_loader.h"
extern "C" int exercise();
int main() { return dnr_load_gui(DnrGuiCef) ? exercise() : 78; }
""")
        executable = self.root / "cef-cpp-driver"
        flags = (self.root / "gui-wrap-flags").read_text().splitlines()
        self.command("c++", "-std=c++20", "-fPIE", "-pie", "-I", NATIVE, "-I", self.root,
                     driver, obj, NATIVE / "gui_loader.cc", self.root / "gui_trampolines.S",
                     *flags, "-ldl", "-o", executable)
        jsc.unlink()
        result = self.run_driver(executable)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("javascriptcore", self.command("readelf", "-d", executable).stdout)

    def test_wrapper_archive_after_import_archive(self):
        # Match Rust's link order: the backend pulls one jump from an import
        # archive, then a later CEF wrapper archive needs another jump. GNU ld
        # does not revisit earlier archives, so all jumps share one object.
        early = self.write("early.c", """
extern long gui_integer(long,long,long,long,long,long,long,long);
extern int late(void);
int exercise(void) {
  if (gui_integer(1,2,3,4,5,6,7,8) != 204) return 1;
  return late();
}
""")
        late = self.write("late.c", """
extern double gui_float(double,double,double,double,double,double,double,double,double);
int late(void) { return gui_float(1,2,3,4,5,6,7,8,9) != 45; }
""")
        objects = []
        for source in (early, late):
            obj = source.with_suffix(".o")
            self.command("cc", "-fPIC", "-c", source, "-o", obj)
            objects.append(obj)
        manifest = self.write("manifest", "".join(f"cef|{obj}\n" for obj in objects)
                              + f"library|{self.library}\n")
        generator.generate(manifest, self.root, "readelf")
        driver = self.write("driver.cc", """
#include "gui_loader.h"
extern "C" int exercise();
int main() { return dnr_load_gui(DnrGuiCef) ? exercise() : 78; }
""")
        loader = self.root / "loader.o"
        jumps = self.root / "jumps.o"
        self.command("c++", "-std=c++20", "-fPIC", "-I", self.root, "-c",
                     NATIVE / "gui_loader.cc", "-o", loader)
        self.command("cc", "-fPIC", "-c", self.root / "gui_trampolines.S", "-o", jumps)
        imports = self.root / "imports.a"
        wrapper = self.root / "wrapper.a"
        self.command("ar", "rcs", imports, loader, jumps)
        self.command("ar", "rcs", wrapper, objects[1])
        executable = self.root / "archive-driver"
        flags = (self.root / "gui-wrap-flags").read_text().splitlines()
        self.command("c++", "-std=c++20", "-fuse-ld=bfd", "-fPIE", "-pie", "-I", NATIVE,
                     driver, objects[0], imports, wrapper, *flags, "-ldl", "-o", executable)
        result = self.run_driver(executable)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
