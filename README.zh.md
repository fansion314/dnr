# dnr

**共享 Deno 运行时，让 CLI 和桌面应用轻装分发。**

[English](README.md) | 简体中文

dnr 将应用与运行时分开发行。安装一次运行时，就可以把 JavaScript、TypeScript、依赖和资源打包成可执行的 `.dnp` 文件分发，无需让每个应用都携带一份 Deno。

项目提供两个工具：

| 工具 | 用途 |
| --- | --- |
| `dnr` | 运行本地 JS/TS 文件和 `.dnp` 应用包，也可查看或解压包内容。 |
| `dnc` | 将准备好的目录打包为 `.dnp`、薄 macOS `.app` 或 Arch/CachyOS 软件包。 |

多个应用共享的是**已安装的运行时文件**，每个应用仍在独立进程中运行。桌面功能通过 [Laufey](https://github.com/littledivy/laufey) 使用系统 WebView，Linux 也可选择系统 CEF。普通脚本和 HTTP 服务不会自动打开窗口。

## 支持的平台

| 平台 | 桌面后端 | 应用分发格式 |
| --- | --- | --- |
| macOS ARM64 | 系统 WebView（WebKit） | `.dnp`、薄 `.app` |
| Linux x86_64 | GTK3 + WebKitGTK 4.1 | `.dnp`、Arch/CachyOS `.pkg.tar.zst` |
| Linux x86_64，Arch/CachyOS system-CEF 变体 | 系统 CEF + GTK3 | `.dnp`、Arch/CachyOS `.pkg.tar.zst` |

运行时静态包含定制 Deno runtime 和 Laufey 后端；系统 Framework、WebView/CEF 库仍由操作系统提供。目前不支持 Windows、macOS Intel 和 Linux ARM64。

macOS 和 Linux 均已有原生运行时与 GUI 验证记录。包内原生插件已在 macOS 和 Linux 复验，Linux 两个后端与 Arch 桌面打包流程均已通过原生验证。各次验证的日期与覆盖范围见[验证记录](VALIDATION.md)。

## 快速上手

先[从源码构建工具](#从源码构建)。在仓库根目录，将输出目录加入当前终端的 PATH：

```sh
export PATH="$PWD/dist:$PATH"
dnr --version
dnc --version
```

运行仓库中的 TypeScript 示例，然后将同一个应用打包并执行：

```sh
dnr examples/hello/main.ts one two

dnc examples/hello --entry main.ts --app-id com.example.hello -o hello.dnp
./hello.dnp one two
```

也可以显式执行 `dnr hello.dnp one two`。参数会传给应用，调用者的工作目录保持不变。

分发时，将 `hello.dnp` 交给使用者，并确保对方的 PATH 中已安装兼容的 `dnr`，无需安装 `dnc`。如果文件传输后丢失可执行权限，运行 `chmod +x hello.dnp`，或直接用 `dnr` 启动。

**应用以完整权限运行。** dnr 不是沙箱，请只运行可信应用。

## 打包自己的应用

准备一个目录，放入入口、本地模块、资源，以及需要的 `node_modules`：

```text
my-app/
├── main.ts
├── assets/
└── node_modules/     # 如有需要
```

```sh
dnc my-app --entry main.ts --app-id com.example.myapp -o my-app.dnp
```

`dnc` 按原样打包文件。请提前完成前端构建和依赖安装：它不分析 import 依赖图、不下载依赖、不转译，也不压缩代码。`dnr` 在运行时转译 TS/TSX/JSX，加载本地模块与准备好的 `node_modules`，不在线获取 npm、JSR 或 HTTP 模块。应用的 `fetch()`、`Deno.serve()` 等网络 API 仍可正常使用。

常用打包选项：

| 选项 | 用途 |
| --- | --- |
| `--include source=destination` | 将外部文件或目录加入指定的包内相对路径。 |
| `--exclude archive/path` | 排除包内文件或目录及其后代。 |
| `--app-id com.example.myapp` | 为应用存储指定稳定身份。 |
| `--force` | 替换已有输出。 |

未指定 `--app-id` 时，打包器取 `deno.json` 或 `package.json` 中的 name，否则使用输入目录名。正式分发时应指定唯一、稳定的 ID，避免无关应用共享存储身份。

### 查看与解压应用包

```sh
dnr tree my-app.dnp
dnr extract my-app.dnp ./my-app-unpacked
```

两个命令都包含隐藏条目和 `.dnr/manifest.json`，且不会执行应用。`tree` 读取包索引，不解压普通文件；`extract` 要求目标目录不存在或为空，内容验证完成后才发布结果，并保留目录、符号链接和普通 Unix 权限。解压得到 ZIP 内容，不包含外部 shell 启动头。

通过 `dnr tree --help`、`dnr extract --help` 或 `dnc --help` 查看命令帮助。运行恰好名为 `tree` 或 `extract` 的脚本时，请写明路径，例如 `dnr ./tree`。

## 桌面应用

运行内置桌面示例：

```sh
dnr examples/desktop/main.ts

# 打包同一个应用，不携带运行时。
dnc examples/desktop --entry main.ts --app-id com.example.desktop -o desktop.dnp
./desktop.dnp
```

示例启动本地 HTTP 服务，创建 `Deno.BrowserWindow`，并通过 `window.bind("greet", ...)` 注册函数。页面使用 `window.bindings.greet(...)` 调用该函数。参见[完整示例](examples/desktop/main.ts)。

需要 GUI 的桌面 API 被调用时，后端才会初始化。窗口、托盘和后台任务共同决定应用生命周期；关闭最后一个窗口不会自动终止仍在运行的服务。示例会在用户关窗时关闭 HTTP 服务。

如需带独立名称和图标的桌面入口，可以创建 JSON desktop manifest，并在目标平台构建：

```sh
# 在 macOS ARM64 上：
dnc prepared --desktop-manifest desktop.json --target macos -o "release/My App.app"

# 在 Arch/CachyOS x86_64 上：
dnc prepared --desktop-manifest desktop.json --target archlinux -o release/my-app.pkg.tar.zst
```

以上命令需要准备好的应用目录，以及填写了入口、app ID、版本、图标和平台配置的 manifest。完整配置与安装步骤见[桌面打包指南](docs/DESKTOP-PACKAGING.md)。

macOS `.app` 使用原生启动器，并进行本地 ad-hoc 签名；Linux 软件包由系统 `makepkg` 生成。两者均不携带或安装 dnr。目前不提供 Developer ID 签名、公证、DMG/PKG 安装器、深链注册或自动更新。

## 文件、资源与原生插件

`.dnp` 由 shell 启动头和 ZIP 组成，普通文件分别使用 Zstd level 6 压缩。运行时按需解压文件，不会在启动时释放整个应用。

- **相对于模块读取资源：** 使用 `new URL("./assets/data.json", import.meta.url)`。文件系统 API 的裸相对路径遵循调用者的工作目录。
- **包内文件只读。** 虚拟文件系统映射在应用包的真实所在目录，优先读取 ZIP，仅当条目不存在时回退磁盘；目录枚举合并两层内容。
- **外部程序需要真实文件。** 这不是操作系统文件系统挂载，子进程无法直接读取内存 VFS，`chdir` 也只能进入真实磁盘目录。
- **解压内容按进程缓存。** 默认内容预算为 256 MiB，并非应用总内存上限；打开的文件句柄在缓存淘汰后仍保留数据。
- **可以打包 Node-API 和 FFI 原生库。** 首次加载时，运行时校验所请求的库并释放到私有系统临时目录，进程内复用；正常结束或宿主处理退出时清理。强制终止或崩溃可能留下临时文件。

原生库必须匹配目标平台、架构与运行时 ABI。它们依赖的操作系统级共享库不会被自动收集或解压。ZIP 外的原生库继续从磁盘加载。纯 JS/TS 应用在代码与依赖均可移植时，可以跨支持的平台使用同一个包。

完整文件系统语义与校验规则见[包格式文档](docs/FORMAT.md)。

## Arch Linux / CachyOS 软件包

固定发行版的 AUR 配方位于 [`packaging/aur`](packaging/aur/README.md)：默认 `dnr` 使用系统 CEF，`dnr-webview` 使用 WebKitGTK。两者都安装 `dnr` 和 `dnc`，只能选择其一。依赖与构建方法见[打包说明](packaging/aur/README.md)。

## 从源码构建

以下命令均在 dnr 仓库根目录执行。需要 Git、当前稳定版 Rust 工具链、C/C++ 编译器、CMake 和平台开发库。构建运行时还会下载匹配的 V8 预编译引擎库，并保留已有 sccache 配置。

- **macOS ARM64：** 安装 Xcode Command Line Tools 和 CMake。
- **Linux x86_64，WebView：** 安装 Clang/libclang、make、pkg-config，以及 GTK3/WebKitGTK 4.1 开发包。
- **Linux x86_64，系统 CEF：** 还需要 Arch/CachyOS 的 `/usr/lib/cef` 布局、CEF headers/wrapper、`FindCEF.cmake`，以及 GTK3/X11/Xi 开发包。参见 [Linux 指南](docs/LINUX.md)。

使用独立目录准备固定版本的上游源码：

```sh
mkdir -p ../dnr-upstream
git clone https://github.com/denoland/deno.git ../dnr-upstream/deno
git -C ../dnr-upstream/deno checkout --detach abd22074e4

git clone https://github.com/littledivy/laufey.git ../dnr-upstream/laufey
git -C ../dnr-upstream/laufey checkout --detach 1fe8787

cargo run --locked -p xtask -- prepare --deno ../dnr-upstream/deno --laufey ../dnr-upstream/laufey
cargo run --locked -p xtask -- build
```

`prepare` 将上游源码复制到 `.upstream/` 并应用本地接入，不修改源仓库；`build` 生成 `dist/dnr` 和 `dist/dnc`。在根工作区直接执行普通 `cargo build` 不会构建完整运行时。

Linux system-CEF 变体使用以下构建命令：

```sh
cargo run --locked -p xtask -- build --backend system-cef
dist/dnr --check-system-cef
```

两种后端会写入同一个 `dist/dnr`。需要同时保留时请分别复制产物；系统 CEF 更新后，应重新执行兼容性检查。

如果只需要打包器：

```sh
cargo build --locked --release -p dnc
# 输出：target/release/dnc
```

单独构建 `dnc` 无需 Deno 源码或 GUI 开发库，但生成桌面包仍需要目标平台的打包工具。

将构建出的两个工具安装到系统路径：

```sh
sudo install -m 0755 dist/dnr dist/dnc /usr/local/bin/
```

## 开发与文档

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace

# 需要已构建的运行时和 C 编译器，显式执行默认忽略的运行时/原生测试。
DNR_BIN="$PWD/dist/dnr" cargo test --locked -p dnr-package --test runtime --test runtime_native -- --ignored

# 需要图形会话，打开窗口并在绑定检查完成后退出。
dist/dnr examples/desktop/smoke.ts
```

普通工作区测试不执行原生运行时测试。桌面打包和平台专项 GUI 检查的额外要求见下列文档。

| 文档 | 内容 |
| --- | --- |
| [桌面打包](docs/DESKTOP-PACKAGING.md) | Manifest、图标、平台工具、安装与打包测试 |
| [包格式](docs/FORMAT.md) | 归档布局、校验和 VFS 语义 |
| [Linux 指南](docs/LINUX.md) | 原生构建、WebView/CEF 与 Wayland 检查 |
| [性能说明](docs/PERFORMANCE.md) | 基准测试及测量边界 |
| [验证记录](VALIDATION.md) | 已执行检查与尚未覆盖的范围 |
| [第三方组件](THIRD_PARTY.md) | 上游来源与许可证声明 |

详细指南与验证记录目前主要使用中文。

## 作者与许可证

作者：**Peilin Fan** — [peilin.fan@foxmail.com](mailto:peilin.fan@foxmail.com)。

dnr 原创代码采用 [MIT 许可证](LICENSE)。第三方组件保留各自的许可证，详见 [THIRD_PARTY.md](THIRD_PARTY.md)。
