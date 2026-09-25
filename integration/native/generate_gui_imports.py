#!/usr/bin/env python3
"""Generate x86-64 SysV tail jumps from actual native ELF references.

The manifest is emitted by CMake: each line is 'group|path', with group
cef/webview/common for objects/archives and library for candidate providers.
No handwritten function signatures or lists of API symbols are needed.
"""
import argparse
import json
import pathlib
import re
import subprocess


def readelf(tool, *args):
    return subprocess.check_output([tool, "--wide", *args], text=True)


def symbols(tool, path, dynamic=False):
    for line in readelf(tool, "--dyn-syms" if dynamic else "--syms", path).splitlines():
        fields = line.split()
        if len(fields) >= 8 and fields[0].endswith(":") and fields[0][:-1].isdigit():
            yield fields[7], fields[3], fields[4], fields[5], fields[6]


def generate(manifest, output, tool):
    groups = {"cef": 1, "webview": 2, "common": 3}
    references = {}
    definitions = set()
    providers = {}
    libraries = {}
    for line in manifest.read_text().splitlines():
        group, path = line.split("|", 1)
        if group == "library":
            path = str(pathlib.Path(path).resolve())
            if path in libraries:
                continue
            dynamic = readelf(tool, "--dynamic", path)
            match = re.search(r"\(SONAME\).*\[(.*?)\]", dynamic)
            if not match:
                raise ValueError(f"GUI provider has no ELF SONAME: {path}")
            soname = match[1]
            # The Arch CEF package lives outside the default loader search path.
            libraries[path] = "/usr/lib/cef/libcef.so" if soname == "libcef.so" else soname
            for name, kind, binding, visibility, section in symbols(tool, path, True):
                if section == "UND" or visibility not in ("DEFAULT", "PROTECTED"):
                    continue
                # Only an unversioned/default-version export can satisfy our
                # unversioned dlsym lookup. Explicit versioned imports fail below.
                if "@" in name and "@@" not in name:
                    continue
                name = name.split("@@")[0]
                providers.setdefault(name, []).append((path, kind, binding))
        else:
            for name, kind, binding, visibility, section in symbols(tool, path):
                if section == "UND":
                    references.setdefault(name, []).append((groups[group], binding))
                elif binding in ("GLOBAL", "WEAK"):
                    definitions.add(name)

    imports = []
    for name, refs in sorted(references.items()):
        if name in definitions:
            continue
        # These backends use the C APIs of GTK/WebKit/CEF. Their C++ runtime
        # references must continue to bind to libstdc++, even when a GUI DSO
        # happens to export the same template instantiation (e.g. JSC exports
        # std::string members). Treating such exports as GUI APIs both changes
        # runtime ownership and makes CEF depend on the WebView stack.
        if name.startswith("_Z"):
            continue
        matches = providers.get(name.split("@")[0], [])
        if not matches:
            # Non-GUI imports (libc/libstdc++/the static runtime) remain regular
            # linker references. An unknown GUI reference fails the final link.
            continue
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            raise ValueError(f"unsupported versioned/mangled GUI import: {name}")
        if any(binding != "GLOBAL" for _, binding in refs):
            raise ValueError(f"unsupported weak GUI import: {name}")
        if len(matches) != 1:
            raise ValueError(f"ambiguous GUI provider for {name}: {matches}")
        path, kind, binding = matches[0]
        if kind != "FUNC" or binding not in ("GLOBAL", "WEAK"):
            raise ValueError(f"unsupported non-function GUI import {name}: {kind}")
        mask = 0
        for group, _ in refs:
            mask |= group
        imports.append((name, path, mask))
    if not imports:
        raise ValueError("no GUI imports found; refusing to emit an empty loader")
    used = sorted({path for _, path, _ in imports})
    indices = {path: index for index, path in enumerate(used)}
    assembly = [".text"]
    declarations = []
    rows = []
    for index, (name, path, mask) in enumerate(imports):
        slot = f"dnr_gui_slot_{index}"
        wrapper = f"__wrap_{name}"
        # A tail jump preserves integer/SSE argument registers, varargs %al,
        # stack arguments, and the caller's return address.
        assembly += [f".globl {wrapper}", f".hidden {wrapper}",
                     f".type {wrapper}, @function", f"{wrapper}:",
                     "endbr64", f"jmp *{slot}(%rip)", f".size {wrapper}, .-{wrapper}"]
        declarations.append(f'extern "C" {{ __attribute__((visibility("hidden"))) void* {slot} = reinterpret_cast<void*>(&dnr_unloaded_gui_import); }}')
        rows.append(f'  {{{json.dumps(name)}, {indices[path]}, {mask}, &{slot}}},')
    assembly += ['.section .note.GNU-stack,"",@progbits']
    include = ['extern "C" [[noreturn]] void dnr_unloaded_gui_import();', *declarations,
               "static DnrGuiLibrary dnr_gui_libraries[] = {"]
    include += [f"  {{{json.dumps(libraries[path])}, nullptr}}," for path in used]
    include += ["};", "static const DnrGuiImport dnr_gui_imports[] = {", *rows, "};"]
    output.mkdir(parents=True, exist_ok=True)
    for name, contents in {
        "gui_trampolines.S": "\n".join(assembly) + "\n",
        "gui_imports.inc": "\n".join(include) + "\n",
        "gui-wrap-flags": "".join(f"-Wl,--wrap={name}\n" for name, _, _ in imports),
        "gui-imports.json": json.dumps([
            {"symbol": name, "library": libraries[path], "backends": mask}
            for name, path, mask in imports], indent=2) + "\n",
    }.items():
        destination = output / name
        destination.write_text(contents)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--readelf", default="readelf")
    args = parser.parse_args()
    generate(args.manifest, args.output, args.readelf)
