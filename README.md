# dnr

**A shared Deno runtime for lightweight CLI and desktop apps.**

English | [简体中文](README.zh.md)

dnr separates your application from its runtime. Install the runtime once, then distribute JavaScript, TypeScript, dependencies, and assets as an executable `.dnp` file—without shipping another copy of Deno with every app.

The project provides two tools:

| Tool | What it does |
| --- | --- |
| `dnr` | Runs local JS/TS files and `.dnp` packages; can also list or extract package contents. |
| `dnc` | Packages a prepared directory into a `.dnp`, a thin macOS `.app`, or an Arch/CachyOS package. |

Apps share the **installed runtime binary**, while each app runs in its own process. Desktop apps use [Laufey](https://github.com/littledivy/laufey) with a system WebView or, on Linux, system CEF. Ordinary scripts and HTTP servers do not open a window.

## Supported platforms

| Platform | Desktop backend | App distribution |
| --- | --- | --- |
| macOS ARM64 | System WebView (WebKit) | `.dnp`, thin `.app` |
| Linux x86_64 | GTK3 + WebKitGTK 4.1 | `.dnp`, Arch/CachyOS `.pkg.tar.zst` |
| Linux x86_64, Arch/CachyOS system-CEF variant | System CEF + GTK3 | `.dnp`, Arch/CachyOS `.pkg.tar.zst` |

The runtime statically includes the customized Deno runtime and Laufey backend; system frameworks and WebView/CEF libraries remain external dependencies. Windows, macOS Intel, and Linux ARM64 are not currently supported targets.

macOS and Linux have native runtime and GUI validation records. The newest native-plugin changes have been revalidated on macOS only; the full Linux desktop-packaging workflow still needs native validation. See [validation records](VALIDATION.md) for the scope and dates of each result.

## Quick start

First [build the tools](#build-from-source). From the repository root, add the output directory to your current shell's PATH:

```sh
export PATH="$PWD/dist:$PATH"
dnr --version
dnc --version
```

Run the included TypeScript example, then package and run the same application:

```sh
dnr examples/hello/main.ts one two

dnc examples/hello --entry main.ts --app-id com.example.hello -o hello.dnp
./hello.dnp one two
```

You can also launch a package explicitly with `dnr hello.dnp one two`. Arguments are forwarded to the application, and the caller's working directory is preserved.

To distribute it, give the recipient `hello.dnp` and ensure a compatible `dnr` is installed on their PATH. Recipients do not need `dnc`. If a transfer removes the executable bit, run `chmod +x hello.dnp` or launch it explicitly with `dnr`.

**Applications run with full permissions.** dnr is not a sandbox; only run applications you trust.

## Package your application

Prepare a directory containing your entry point, local modules, assets, and any required `node_modules`:

```text
my-app/
├── main.ts
├── assets/
└── node_modules/     # if needed
```

```sh
dnc my-app --entry main.ts --app-id com.example.myapp -o my-app.dnp
```

`dnc` packages files as they are. Run your frontend build and install dependencies beforehand: it does not resolve an import graph, download dependencies, transpile, or minify your code. `dnr` transpiles TS/TSX/JSX at runtime and loads local modules and prepared `node_modules`; it does not fetch npm, JSR, or HTTP modules. Application networking such as `fetch()` and `Deno.serve()` remains available.

Useful packaging options:

| Option | Purpose |
| --- | --- |
| `--include source=destination` | Add an external file or directory at a package-relative path. |
| `--exclude archive/path` | Exclude a file or directory and its descendants. |
| `--app-id com.example.myapp` | Set a stable identity for application storage. |
| `--force` | Replace an existing output. |

Without `--app-id`, the packager uses the name in `deno.json` or `package.json`, falling back to the input directory name. Use a unique, stable ID for distributed apps so unrelated applications do not share a storage identity.

### Inspect or extract a package

```sh
dnr tree my-app.dnp
dnr extract my-app.dnp ./my-app-unpacked
```

Both commands include hidden entries and `.dnr/manifest.json`, and neither executes the app. `tree` reads the package index without decompressing ordinary files. `extract` requires a new or empty directory, verifies file contents before publishing the result, and preserves directories, symlinks, and ordinary Unix permissions. It extracts the ZIP contents, not the shell launcher header.

Use `dnr tree --help`, `dnr extract --help`, or `dnc --help` for command help. To run a script named `tree` or `extract`, use an explicit path such as `dnr ./tree`.

## Desktop apps

Try the included desktop example:

```sh
dnr examples/desktop/main.ts

# Package the same app without including the runtime.
dnc examples/desktop --entry main.ts --app-id com.example.desktop -o desktop.dnp
./desktop.dnp
```

The example starts a local HTTP server, creates a `Deno.BrowserWindow`, and registers a function with `window.bind("greet", ...)`. The page calls it through `window.bindings.greet(...)`. See [the complete example](examples/desktop/main.ts).

GUI initialization happens when a desktop API needs it. Windows, trays, and background tasks contribute to the app's lifetime; closing the last window does not automatically stop an active server. The example shuts its server down when the user closes the window.

For a desktop launcher with an application name and icon, create a JSON desktop manifest and build on the target platform:

```sh
# On macOS ARM64:
dnc prepared --desktop-manifest desktop.json --target macos -o "release/My App.app"

# On Arch/CachyOS x86_64:
dnc prepared --desktop-manifest desktop.json --target archlinux -o release/my-app.pkg.tar.zst
```

These commands assume a prepared application directory and a manifest with the entry point, app ID, version, icon, and platform settings. Follow the [desktop packaging guide](docs/DESKTOP-PACKAGING.md) for the complete manifest and installation steps.

The macOS bundle uses a native launcher and local ad-hoc signing. Linux packages are generated by system `makepkg`. Neither format bundles or installs dnr. Developer ID signing, notarization, DMG/PKG installers, deep-link registration, and automatic updates are not provided.

## Files, resources, and native plugins

A `.dnp` is a shell launcher followed by a ZIP archive, with ordinary files compressed individually using Zstd level 6. Files are decompressed on demand rather than unpacking the entire application at startup.

- **Read resources relative to their module:** use `new URL("./assets/data.json", import.meta.url)`. Bare relative filesystem paths use the caller's working directory.
- **Packaged files are read-only.** The package's virtual filesystem is mapped to the package's real directory. ZIP entries take priority; only missing entries fall back to disk. Directory listings merge both layers.
- **External programs need real files.** This is not an OS filesystem mount. Subprocesses cannot read the in-memory VFS, and `chdir` requires a real disk directory.
- **Decompressed data is cached per process.** The default content budget is 256 MiB; this is not a limit on total application memory. Open file handles retain their data even after cache eviction.
- **Node-API and FFI libraries can be packaged.** On first load, requested native libraries are verified and extracted to a private system temporary directory, reused within the process, and cleaned up on normal or host-handled exit. Forced termination or a crash can leave temporary files.

Native libraries must match the platform, architecture, and runtime ABI. Their OS-level shared-library dependencies are not collected or extracted automatically. Libraries outside the ZIP load from disk as usual. Pure JS/TS packages can be reused across supported platforms when their code and dependencies are portable.

See the [package format](docs/FORMAT.md) for precise filesystem and integrity rules.

## Build from source

Run the following commands from the dnr repository root. You need Git, a current stable Rust toolchain, a C/C++ compiler, CMake, and platform development libraries. The runtime build also downloads the matching prebuilt V8 engine library. Existing sccache configuration is preserved.

- **macOS ARM64:** install Xcode Command Line Tools and CMake.
- **Linux x86_64, WebView:** install Clang/libclang, make, pkg-config, and GTK3/WebKitGTK 4.1 development packages.
- **Linux x86_64, system CEF:** additionally needs the Arch/CachyOS CEF layout under `/usr/lib/cef`, CEF headers/wrapper and `FindCEF.cmake`, plus GTK3/X11/Xi development packages. See the [Linux guide](docs/LINUX.md).

Prepare dedicated upstream checkouts at the pinned revisions:

```sh
mkdir -p ../dnr-upstream
git clone https://github.com/denoland/deno.git ../dnr-upstream/deno
git -C ../dnr-upstream/deno checkout --detach abd22074e4

git clone https://github.com/littledivy/laufey.git ../dnr-upstream/laufey
git -C ../dnr-upstream/laufey checkout --detach 1fe8787

cargo run --locked -p xtask -- prepare --deno ../dnr-upstream/deno --laufey ../dnr-upstream/laufey
cargo run --locked -p xtask -- build
```

`prepare` copies those sources into `.upstream/` and applies the local integration without modifying the source checkouts. `build` produces `dist/dnr` and `dist/dnc`. A regular root-workspace `cargo build` does not build the full runtime.

For the Linux system-CEF variant, replace the build command with:

```sh
cargo run --locked -p xtask -- build --backend system-cef
dist/dnr --check-system-cef
```

Both variants write to the same `dist/dnr`. Keep separate copies if you need both, and repeat the CEF compatibility check after a system CEF update.

If you only need the packager:

```sh
cargo build --locked --release -p dnc
# Output: target/release/dnc
```

Building `dnc` alone does not require the Deno checkout or GUI development libraries. Generating desktop bundles still requires the target platform's packaging tools.

For a system-wide installation of both built tools:

```sh
sudo install -m 0755 dist/dnr dist/dnc /usr/local/bin/
```

## Development and documentation

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace

# Requires a built runtime and C compiler; runs the ignored runtime/native tests.
DNR_BIN="$PWD/dist/dnr" cargo test --locked -p dnr-package --test runtime --test runtime_native -- --ignored

# Requires a graphical session; opens a window and exits after the binding check.
dist/dnr examples/desktop/smoke.ts
```

Ordinary workspace tests do not run the native runtime tests. Desktop-packaging checks and platform-specific GUI checks have additional requirements documented below.

| Document | Contents |
| --- | --- |
| [Desktop packaging](docs/DESKTOP-PACKAGING.md) | Manifest, icons, platform tools, installation, and packaging tests |
| [Package format](docs/FORMAT.md) | Archive layout, validation, and VFS semantics |
| [Linux guide](docs/LINUX.md) | Native builds, WebView/CEF, and Wayland checks |
| [Performance](docs/PERFORMANCE.md) | Benchmarks and measurement boundaries |
| [Validation records](VALIDATION.md) | Executed checks and remaining coverage gaps |
| [Third-party components](THIRD_PARTY.md) | Upstream sources and license notices |

The detailed guides and validation records are currently primarily in Chinese.

## Author and license

Created by **Peilin Fan** — [peilin.fan@foxmail.com](mailto:peilin.fan@foxmail.com).

dnr's original code is released under the [MIT License](LICENSE). Third-party components retain their respective licenses; see [THIRD_PARTY.md](THIRD_PARTY.md).
