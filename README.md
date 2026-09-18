# dnr

共享安装的 Deno runtime，以及不携带运行时的可执行应用包。

- `dnr`：运行本地 JS/TS 或 `.dnp` 应用包；调用 GUI API 时启动桌面后端。
- `dnc`：将准备好的目录打包成 shell 启动头 + ZIP，文件使用 Zstd level 6。
- 支持目标：macOS ARM64 WebView、Linux x86_64 WebView、Linux x86_64 system-CEF。

## 构建

需要 Rust、平台 C/C++ 编译器、CMake。macOS 需要 Xcode Command Line Tools；Linux WebView 需要 GTK3 和 WebKitGTK 4.1 开发包。

```sh
cargo run -p xtask -- prepare --deno ../mirror/deno --laufey ../mirror/laufey
cargo run -p xtask -- build
# Linux x86_64 / Arch 系统 CEF：
cargo run -p xtask -- build --backend system-cef
```

上游快照复制到 `.upstream/` 并应用 `integration/*.patch`；不修改源仓库。版本固定为 Deno `abd22074e4`、Laufey `1fe8787`。定制 runtime 在本地构建，V8 使用其匹配的预编译引擎库。构建保留已有的 sccache 配置。

`dist/dnr` 和 `dist/dnc` 是两个独立工具。将它们放进 PATH 即可。dnr 本体静态包含 Laufey 桥接和 Deno runtime，系统 WebView/CEF 库仍由操作系统提供。只构建打包器可执行 `cargo build --release -p dnc`，不需要准备 Deno 源码或原生 GUI 依赖。

## 使用

```sh
dnr examples/hello/main.ts one two
dnc examples/hello --entry main.ts -o hello.dnp
./hello.dnp one two
dnr hello.dnp one two

dnc examples/desktop --entry main.ts -o desktop.dnp
./desktop.dnp
```

打包器不分析 import、不安装依赖、不运行前端构建。请提前准备好本地源码、资源与 `node_modules`。可以通过 `--include source=destination` 添加外部资源，通过 `--exclude archive/path` 排除文件或目录。覆盖输出需要 `--force`。

应用全权限运行，不是沙箱。npm/JSR/HTTP 模块不在线下载；网络 API 如 `fetch`、`Deno.serve` 可正常使用。原生库必须存在于磁盘，不会从应用 ZIP 释放原生库。

桌面页面通过 `window.bindings.<name>(...)` 调用 `BrowserWindow.bind()` 注册的函数。`Deno.serve()` 本身不会创建窗口。GUI 示例使用显式 `BrowserWindow`；窗口关闭后，应用需要关闭仍运行的 HTTP 服务或其他任务。

系统签名和应用身份属于 dnr，不提供每个包独立的 macOS `.app`、通知身份或深链安装。包格式见 [FORMAT.md](docs/FORMAT.md)，上游来源见 [THIRD_PARTY.md](THIRD_PARTY.md)。

## 文件系统语义

- ZIP 挂载在应用包所在目录，先查 ZIP，只有不存在才回退磁盘。
- 保留调用者 cwd。资源使用 `new URL('./resource', import.meta.url)`，裸相对路径遵循 cwd。
- ZIP 内节点只读；目录枚举合并 ZIP 和磁盘内容，同名条目以 ZIP 为准。
- 目录和元数据查询不解压普通文件；文件首次读取时解压，后续共享进程内 LRU 缓存（默认 256 MiB）。
- 缓存预算不包含仍在使用的文件句柄、JS 字符串和引擎堆。
- 外部进程不认识这个 VFS；调用外部工具时只能传递真实文件。
- `chdir` 只能进入真实磁盘目录。仅存在于 ZIP 的目录不是 OS 挂载点，会明确报错；资源定位使用 `import.meta.url`。
- `--app-id` 控制应用存储身份，默认取配置中的 name 或输入目录名。同名应用应显式指定不同 ID。

## 验证

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
# 完整原生运行测试（包含磁盘 Node-API 扩展测试）：
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored
# 打开测试窗口，完成页面绑定检查后自动退出：
dist/dnr examples/desktop/smoke.ts
```

平台验证以实际运行结果为准；Linux GUI、Wayland 和 system-CEF 需要原生 Linux 环境，不能用 macOS 上的文件检查替代。当前实现与验证状态见 [验证记录](VALIDATION.md)，延后的 Linux 验收步骤见 [LINUX.md](docs/LINUX.md)。
