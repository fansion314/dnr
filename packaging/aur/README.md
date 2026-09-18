# Arch Linux / CachyOS packages

Four release-pinned recipes are provided for native x86_64 systems:

| Package | Backend | Backend dependencies |
| --- | --- | --- |
| `dnr` (default) | system CEF | `cef`, `gtk3`, `libx11`, `libxi` |
| `dnr-webview` | WebKitGTK | `webkit2gtk-4.1`, `gtk3`, `libsoup3` |
| `dnr-bin` | system CEF, GitHub Release binary | Same as `dnr` |
| `dnr-webview-bin` | WebKitGTK, GitHub Release binary | Same as `dnr-webview` |

All install `/usr/bin/dnr` and `/usr/bin/dnc`, licenses and documentation.
They conflict with each other; the alternative packages provide `dnr=0.1.0` so applications
can depend on either runtime. Applications requiring CEF must still check the backend.
The source URL is `https://github.com/fansion314/dnr.git`, pinned to `v0.1.0`.
Deno and Laufey are downloaded separately at the full commits used by xtask.
These are release packages, not moving `-git` packages.

## Build and install

To install prebuilt binaries without compiling Rust, Deno or V8:

```sh
cd packaging/aur/dnr-bin         # or ../dnr-webview-bin
makepkg -si
# Alternatively: paru -Bi .
```

The `-bin` recipes download the matching `.pkg.tar.zst` and its `.sha256` from
the GitHub Release. `prepare()` verifies the exact archive name and SHA-256
before extracting its payload; the package-manager metadata is regenerated for
the `-bin` package name. Checksums are supplied by the same HTTPS release, not
independently signed or hardcoded in the tag: CI produces the binary after the
tagged recipes already exist. Do not bypass `prepare()` when building them.

For source builds:

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

## GitHub release builds

`.github/workflows/release-arch.yml` runs only when a `vMAJOR.MINOR.PATCH` tag
is pushed. Branch pushes, pull requests and manual dispatch do not start it.
The two matrix jobs use the official `ghcr.io/archlinux/archlinux:base-devel`
image on x86_64 hosted runners. Each job installs declared dependencies, checks
out pinned upstream commits, builds as an unprivileged user and runs the source
recipe's workspace and runtime/native tests. Release compiler flags target
generic x86_64; no `-march=native` or `target-cpu=native` is used.

Only after both backends succeed does the release job create/update the matching
GitHub Release and upload the two pacman packages, SHA-256 files and build
environment records. Failed builds retain their logs as workflow artifacts.
An old run refuses to publish if its tag has since moved to another commit.
Cargo downloads are cached; the first build still compiles the custom Deno
runtime, while V8 and system GUI libraries are prebuilt dependencies.

The headless CI checks do not replace real desktop/Wayland GUI validation.
See [Linux validation](../../docs/LINUX.md) for those checks.
