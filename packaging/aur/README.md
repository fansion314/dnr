# Arch Linux / CachyOS packages

Two release-pinned recipes are provided for native x86_64 builds:

| Package | Backend | Backend dependencies |
| --- | --- | --- |
| `dnr` (default) | system CEF | `cef`, `gtk3`, `libx11`, `libxi` |
| `dnr-webview` | WebKitGTK | `webkit2gtk-4.1`, `gtk3`, `libsoup3` |

Both install `/usr/bin/dnr` and `/usr/bin/dnc`, licenses and documentation.
They conflict with each other; `dnr-webview` provides `dnr=0.1.0` so applications
can depend on either runtime. Applications requiring CEF must still check the backend.
The source URL is `https://github.com/fansion314/dnr.git`, pinned to `v0.1.0`.
Deno and Laufey are downloaded separately at the full commits used by xtask.
These are release packages, not moving `-git` packages.

## Build and install

Install `base-devel` and choose one recipe, then run as a normal user:

```sh
sudo pacman -S --needed base-devel git
# From this repository:
cd packaging/aur/dnr             # or ../dnr-webview
makepkg -si
```

`makepkg -s` installs missing repository dependencies. The recipes declare Rust,
Clang/libclang, CMake, Python, pkgconf and Git for building, plus the directly
linked system libraries. Make, GCC and binutils are supplied by `base-devel`.
Existing sccache configuration is preserved; sccache is not required.
Cargo uses both committed lockfiles. The build needs network access for Cargo
sources and the matching prebuilt Rusty V8 library; V8 itself is not built locally.
The complete runtime build can take substantial time and disk space.

Install `libayatana-appindicator` for tray support and `libnotify` for desktop
notifications (`notify-send`). Install `base-devel` on the
target machine if using `dnc --target archlinux`, which invokes system makepkg.
Neither is needed for ordinary CLI scripts. The system-CEF recipe requires the
Arch CEF headers, wrapper sources, `FindCEF.cmake` and resources under `/usr/lib/cef`.
It checks CEF API 14900 during `check()`; a version number alone cannot guarantee
ABI compatibility. After a system CEF update, run `dnr --check-system-cef` and
rebuild if the compatibility check fails.

`check()` runs workspace tests and all 14 runtime/Node-API/FFI tests without
opening GUI windows. GUI and platform-specific desktop-bundle tests are separate;
see [VALIDATION.md](../../VALIDATION.md) and [docs/LINUX.md](../../docs/LINUX.md).

If a manually installed `/usr/local/bin/dnr` or `dnc` exists, it may precede
`/usr/bin` on PATH. Check `command -v dnr dnc` after installation and migrate
those manual files deliberately; pacman does not manage them.

## AUR metadata

Each package directory is self-contained and contains `PKGBUILD` and `.SRCINFO`.
To refresh metadata after editing a recipe:

```sh
makepkg --printsrcinfo > .SRCINFO
```

The recipes follow the [PKGBUILD manual](https://man.archlinux.org/man/PKGBUILD.5.en).
They are ready for submission to separate AUR repositories; including these files
in GitHub does not itself register an AUR package.
