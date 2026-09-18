# dnr 项目工作指南

## 项目介绍

dnr 将运行时与应用内容分开发行，避免每个 Deno CLI/桌面应用重复携带完整运行时。

- `dnr`：共享安装的原生可执行文件，执行磁盘 JS/TS 或 `.dnp` 应用包。
- `dnc`：将准备好的目录和资源打包为可直接执行的 `.dnp`，不携带运行时。
- “共享”指共享安装文件；每个应用仍运行在独立进程中，不是常驻的多应用服务。
- 目标平台：macOS ARM64、Linux x86_64。默认使用系统 WebView，Linux 另有 system-CEF 构建变体。
- 主体、包格式和 VFS 用 Rust；复用并少量修改 Laufey 的 C++/Objective-C++ 后端。

先读 [README.md](README.md)、[包格式](docs/FORMAT.md) 和 [验证记录](VALIDATION.md)。Linux 操作细节另见 [LINUX.md](docs/LINUX.md)。

## 已确定的设定

1. dnr 本地构建定制 Deno runtime，并将 Laufey 后端静态链接进同一个可执行文件。V8 使用匹配的预编译引擎库。
2. 单文件要求适用于 runtime 和后端：启动时不释放 libdenort 或 Laufey 原生组件。系统 Framework、GTK/WebKitGTK、系统 CEF 仍是外部系统依赖。
3. 应用格式是 shell 启动头 + ZIP；普通文件按条目使用 Zstd level 6。启动头转发参数，manifest 保存入口、格式版本和 appId。
4. dnc 只打包目录与显式 include/exclude 的资源，不执行前端构建、依赖安装、依赖图收集、转译或 minify。
5. dnr 支持本地模块和准备好的 `node_modules`，运行时转译 TS/TSX/JSX；不在线获取 npm、JSR、HTTP 模块。应用自己的 `fetch`、HTTP 服务等网络 API 不受此范围限制。
6. 应用默认全权限运行，不是沙箱。兼容的原生扩展只从磁盘加载，ZIP 内原生库必须明确拒绝，不能偷偷落盘。
7. 实际调用需要 GUI 的桌面 API 时才启动后端。普通脚本、HTTP 服务和 CLI 异常不应打开窗口。
8. 页面通过 `window.bindings.<name>()` 调用 `BrowserWindow.bind()`。窗口、托盘、后台 JS 任务和退出事件共同决定生命周期；不能在最后一个窗口关闭时直接终止仍有工作的应用。
9. 不包含每应用独立的 macOS bundle 身份、安装器、深链注册或自动更新。变更这些边界需要用户明确提出。

### VFS 不变量

- ZIP 映射在应用包真实所在目录；ZIP 优先，仅“不存在”时回退磁盘。损坏、CRC 或解压错误不能触发回退。
- ZIP 节点只读，目录枚举合并两层，同名条目以 ZIP 为准。
- 启动和元数据查询不解压普通文件；按文件懒解压并共享进程内缓存，默认内容预算 256 MiB。
- 打开的文件句柄保留已解压数据，LRU 淘汰不能使分段读取重复解压或失效。
- 启动保留调用者 cwd；模块相对资源使用 `import.meta.url`。显式 `chdir` 仅能进入真实磁盘目录，ZIP-only 目录报错。
- 这不是 OS 文件系统挂载；外部进程不能直接读取内存 VFS。
- 保留并验证包内符号链接；拒绝越界、循环、重复路径和文件/目录冲突。
- 应用存储按 appId 隔离，不能让所有应用继承同一个 dnr 存储身份。

## 代码导航与修改位置

| 位置 | 职责 |
| --- | --- |
| `crates/package/` | 包格式、目录收集、ZIP 读写、路径检查和解压缓存；不依赖 Deno/GUI |
| `crates/dnc/` | 打包器 CLI 参数与输出 |
| `integration/rt/` | dnr 启动、应用元数据、桌面生命周期和原生构建脚本 |
| `integration/native/` | 静态 Laufey 接入、CMake、系统 CEF ABI 检查 |
| `integration/deno.patch` | Deno 模块加载、文件系统、原生库边界和事件循环改造 |
| `integration/laufey.patch` | 后端静态接入、懒初始化和原生生命周期改造 |
| `xtask/` | 准备上游源码、应用补丁、同步接入代码、构建及精简签名产物 |
| `crates/package/tests/` | 包单元/集成测试及需显式运行的原生测试 |
| `examples/` | CLI 和真实桌面烟雾测试 |

应用执行路径：CLI 解析 → 磁盘入口或 ZIP manifest → 模块解析/VFS → Deno worker；首次 GUI 调用经主线程初始化 Laufey，绑定事件由 runtime 线程处理。

`.upstream/`、`target/`、`dist/` 是准备或生成目录，不是实现的唯一来源。不要只在其中修复问题：runtime 接入改动保存到 `integration/rt/`，上游改动保存到对应补丁，依赖改动保存到锁文件。

`xtask build` 会同步接入源码和锁文件，但不会自动应用新补丁。修改补丁后要验证干净快照上的 `prepare`；已有旧补丁的缓存树可能需要重新准备。替换任务自身的缓存前，先确保调试改动已保存。

## 开发与构建约定

- 不修改用户提供的 mirror 源仓库。当前固定 Deno `abd22074e4`、Laufey `1fe8787`；升级时同时核对补丁、snapshot、Laufey C ABI、CEF API 和测试。
- 保留 `Cargo.lock` 和 `integration/deno.Cargo.lock`。新增依赖尽量使用兼容的最新稳定版，不因“最新”破坏固定上游的兼容关系。
- 保持 sccache 默认启用；遇到沙箱权限问题申请提权，不清空 `RUSTC_WRAPPER` 绕过。
- 遵循宿主环境的 RTK 约定。下面展示原生命令以便跨机器复用；执行时按环境要求加 `rtk` 或 `rtk proxy`。
- 未经用户主动要求，不自动采用本地 ExecPlan/plan-doc 流程；不自动提交或推送。
- 若新增独立前端项目，默认使用 Node、pnpm、React、Vite+（`vp`）。

在项目根目录执行：

```sh
# 默认从 ../mirror/deno 和 ../mirror/laufey 读取；也可显式指定独立源码检出。
cargo run -p xtask -- prepare --deno /path/to/deno --laufey /path/to/laufey
cargo run -p xtask -- build
# 迭代时可构建 debug；也会覆盖 dist/ 下的对应工具。
cargo run -p xtask -- build --debug
```

输出是 `dist/dnr`、`dist/dnc`。根工作区的普通 `cargo build/test` 不会构建完整 dnr；运行时通过 xtask 在准备后的 Deno 工作区中构建。仅构建 dnc 可用 `cargo build --release -p dnc`。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored
```

原生测试默认被忽略；普通 `cargo test --workspace` 通过不能代表 runtime 已通过。原生测试还需要 C 编译器，会构建真实 Node-API 插件。文档修改检查路径和命令即可，不必重建 runtime。

## Linux 验证的接续任务

截至 2026-09-18，macOS ARM64 已完成 release、12 项包测试、4 项原生测试和真实 WebView 自动验证。Linux 原生构建及 GUI 验收尚未执行，不能继承 macOS 的“通过”结论。

用户明确要求 Linux 验收留待之后进行。后续只有在用户要求开始 Linux 验证并指定环境时再执行；不要自行在当前 Mac 上交叉构建冒充验收，也不要自行选择远程主机安装系统依赖。此前只读检查过 yama-ts，它当时缺少所需 GUI 依赖和图形会话，这不是长期有效的环境结论。

### 1. 准备原生环境

- 使用 Linux x86_64，先记录发行版、`uname -sm`、Rust/编译器版本、图形会话类型和 GPU/驱动。
- 传输项目源码与锁文件；不要复用 Mac 的 `.upstream/`、`target/`、`dist/`，尤其不要把 `.upstream/build-tools/` 中的 macOS 工具带入 Linux 构建。
- 准备上述固定提交的独立 Deno/Laufey 检出，不通过切换用户已有 mirror 的分支来满足版本。
- 检查 Git、稳定 Rust、C/C++ 编译器、libclang、CMake、make、pkg-config、strip。保留已配置的 sccache。
- CLI/ABI 检查可在无显示环境运行，但仍需要该变体的系统动态库；GUI 验收需要真实桌面会话。

### 2. WebView 构建与测试

```sh
uname -sm
rustc --version
pkg-config --modversion gtk+-3.0 webkit2gtk-4.1
cargo run -p xtask -- prepare --deno /path/to/deno --laufey /path/to/laufey
cargo run -p xtask -- build
file dist/dnr
ldd dist/dnr
cargo test --workspace
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored
dist/dnr examples/desktop/smoke.ts
```

确认 ELF x86-64、没有缺失动态库、原生测试全部通过；桌面测试应打开窗口，关窗后完成异步任务，输出 `DNR_GUI_OK`，退出码为 0。

### 3. 验证应用包和 Wayland

```sh
dist/dnc examples/hello --entry main.ts -o dist/hello.dnp --force
dist/dnc examples/desktop --entry smoke.ts -o dist/desktop-smoke.dnp --force
dnr_project_root="$PWD"
export PATH="$dnr_project_root/dist:$PATH"
(
  cd /tmp
  "$dnr_project_root/dist/hello.dnp" "hello world" "" --literal
  "$dnr_project_root/dist/desktop-smoke.dnp"
)
```

确认参数、调用者 cwd、ZIP 资源和磁盘回退一致。再在真实 Wayland/NVIDIA 会话中复验：dnr 会在初始化后端前设置 `__NV_DISABLE_EXPLICIT_SYNC=1`，其实际效果需要实机证据。若出现 Wayland 协议错误，记录 compositor、驱动及必要的 `WAYLAND_DEBUG=1` 日志，不要只凭服务端开始监听就宣布 GUI 正常。

补充检查托盘保活、最后一个窗口关闭后的后台任务、多应用并行和应用存储隔离。没有相应环境或示例覆盖时，在结果中明确标为未验证。

### 4. system-CEF 变体

使用原生 Linux x86_64 的 Arch/CachyOS 系统 CEF 布局。需要 `/usr/lib/cef` 中的库与资源、CEF headers/wrapper、`FindCEF.cmake`，以及 GTK3/X11/Xi 开发包。当前 CEF API 固定为 14900。

```sh
pkg-config --modversion gtk+-3.0 xi x11
cargo run -p xtask -- build --backend system-cef
dist/dnr --check-system-cef
ldd dist/dnr
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored
dist/dnr examples/desktop/smoke.ts
PATH="$PWD/dist:$PATH" dist/desktop-smoke.dnp
```

确认 API hash 检查通过、`libcef.so` 来自 `/usr/lib/cef`、资源可定位、GUI 和绑定可用、退出后无遗留 CEF 子进程。不能跳过 ABI 校验，不能通过复制整个 Chromium 来掩盖 system-CEF 问题。

两个后端的构建会覆盖同一个 `dist/dnr`；分别记录版本、依赖和结果，保留产物时放进各自目录，避免把 WebView 的结果写成 CEF 的结果。

### 5. 收尾与证据

将平台、后端、工具链、系统库/CEF 版本、命令、退出码及真实 GUI 结果写入 `VALIDATION.md`；必要时同步 `docs/LINUX.md`。区分编译成功、自动测试通过和手工 GUI 观察。未执行、失败或受环境限制的检查必须保留状态，不能把“预计支持”写成“验证通过”。
