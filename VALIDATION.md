# 验证记录

以下 macOS 记录为此前验收结果；本轮 Linux 改动后未在 macOS 重新执行。Linux 记录见本文后半部分。

环境：macOS ARM64，Rust 1.98.1；Deno `abd22074e4`、Laufey `1fe8787`。sccache 保持启用。

## 已完成

- `cargo test --workspace`：12 项包格式/缓存测试通过；原生测试默认显式忽略，需按下面命令执行。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo fmt --all -- --check`：通过。
- 最终精简并签名的 release 产物上，4 项原生集成测试全部通过，覆盖磁盘脚本和 ZIP 应用、TS、import map、CommonJS npm exports、Worker、磁盘回退、合并目录、只读保护、seek/EOF、特殊字符入口和参数。
- 真实 Node-API 插件在磁盘上可加载；相同插件放进 ZIP 时明确拒绝，不释放到磁盘；排除后部署到包旁边可通过磁盘回退加载。
- CLI HTTP 服务不创建窗口；异步错误正常失败；Node `beforeExit` 可以调度并完成异步任务。
- 原生 WebView 自动测试已验证页面标题、页面调用 Deno binding、结果返回页面以及关窗后异步任务完成。成功标记为 `DNR_GUI_OK`，退出码为 0。
- Mach-O ARM64 产物经 `otool -L` 检查，只依赖系统动态库/Framework，没有随附的 libdenort 或 Laufey 动态库依赖。
- macOS ad-hoc 签名经 `codesign --verify --verbose dist/dnr` 验证。
- 两份源码补丁均通过反向 `git apply --check`，mirror 仓库保持干净；构建使用单独准备的源码和锁定依赖。

## 最终回归命令

```sh
cargo run -p xtask -- prepare
cargo run -p xtask -- build
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored
dist/dnc examples/desktop --entry smoke.ts -o dist/desktop-smoke.dnp --force
PATH="$PWD/dist:$PATH" dist/desktop-smoke.dnp
```

`chdir` 边界已通过 release 回归：真实目录正常切换，ZIP-only 目录明确报错，不伪造 OS 工作目录。最终桌面包从 `/private/tmp` 直接启动，再次得到 `DNR_GUI_OK` 和退出码 0。

## 产物

| 文件 | 大小 | 内容 |
| --- | --- | --- |
| `dist/dnr` | 约 71 MiB | macOS ARM64 release，共享运行时，已精简符号并 ad-hoc 签名 |
| `dist/dnc` | 约 1.8 MiB | macOS ARM64 release 打包器 |
| `dist/hello.dnp` | 783 字节 | CLI 示例，含脚本与文本资源 |
| `dist/desktop-smoke.dnp` | 1,798 字节 | 原生窗口、双向绑定及关窗后异步任务测试 |

## 原有范围

此前延后的 Linux 验收已于 2026-09-18 在用户指定的本机继续执行，结果如下。复验步骤见 [LINUX.md](docs/LINUX.md)。

首版不包含 npm/JSR/HTTP 模块在线安装、ZIP 原生库释放、桌面安装器、每应用独立的 macOS bundle 身份或自动更新。这些是已约定的范围边界。


## Linux x86_64（2026-09-18）

### 环境与源码

- 本机 `fansionia-kaze`，CachyOS，`Linux x86_64`，内核 `7.2.6-1-cachyos-bore-lto`。
- Rust/Cargo 1.98.1，GCC 16.2.1，Clang/libclang 22.1.8，CMake 4.4.3。
- KDE / KWin 6.7.5 Wayland，`DISPLAY=:0`、`WAYLAND_DISPLAY=wayland-0`。
- NVIDIA GeForce RTX 3080，NVIDIA open kernel driver / nvidia-utils 615.71.09。
- GTK3 3.24.52、WebKitGTK 4.1 API / 2.52.6、Xi 1.8.3、X11 1.8.13；托盘使用系统 Ayatana AppIndicator。
- 系统 CEF `152.0.6+g708dc14+chromium-152.0.7977.83`，库与资源位于 `/usr/lib/cef`；使用固定 API 14900，Linux API hash 为 `778f64e58ff024840e29fb49bb7b9c3819f12191`。
- 独立克隆至 `../mirror/deno` 与 `../mirror/laufey`，分别检出 `abd22074e47c6a5cd14e9e4e84743f084aa5a575`、`1fe87874288e8359fa3de04d18cc14f56957b000`；两者工作区保持干净。
- 从 Linux 原生源码构建，未复用 macOS 生成目录；V8 使用匹配的 `150.4.0` 预编译库。按用户要求，最终构建不使用 sccache，`CARGO_BUILD_JOBS=8`。

### 本轮修复

- 将 GUI 正常退出移回主线程：首次 WebView 烟雾测试曾在输出成功标记后 SIGABRT，core 显示 `dnr-runtime` 线程在 `exit()` 中执行 WebKit 清理。现在先退出原生事件循环，再由主线程清理并退出。
- `Deno.exit()` / Node `process.exit()` 使用 Deno 的 isolate 终止机制，将请求的退出码交回宿主；CEF 等待所有浏览器关闭后再执行 `CefShutdown()`。
- 修复条件块内顶层 await 被误判为 CommonJS 的问题；新增测试覆盖磁盘与 ZIP 中的 `.ts` / `.js`，旧产物上已确认该测试失败。
- 接入 CEF 的主 frame 加载完成事件；桌面示例忽略创建窗口时的初始 `about:blank`，再验证实际页面。
- CEF 浏览器进程显式初始化 GTK3，为原生托盘、菜单和对话框提供显示上下文；CEF 子进程和普通 CLI 不执行此初始化。
- CEF 在页面切换、窗口关闭时结束对应的待处理 JS 求值，防止丢失 renderer 回应后 Promise 永久保活；托盘回归包含关窗时的 16 个未完成 `executeJs()` 请求。
- CEF 添加 `--no-first-run` 与 `--no-default-browser-check`，禁用 Chrome 首次运行及默认浏览器确认流程。
- 新增 `examples/desktop/tray-smoke.ts`、`examples/desktop/exit-smoke.ts`，复验托盘独立保活、GUI 显式退出及异常退出。

### 验证结果

下表对应本轮最终 release 产物。两个后端均在真实 KDE Wayland / NVIDIA 会话完成自动 GUI 验证，无需处理 Chrome 首次运行确认窗口。

| 检查 | WebView | system-CEF |
| --- | --- | --- |
| 原生 release、ELF x86-64、动态依赖无缺失 | 通过 | 通过，`libcef.so` 来自 `/usr/lib/cef` |
| 系统 CEF API hash 检查 | 不适用 | API 14900 匹配，退出码 0 |
| 原生集成测试（6 项，含显式退出时仍有活动服务） | 全部通过，退出码 0 | 全部通过，退出码 0 |
| 磁盘 GUI、页面标题、双向绑定、关窗后异步任务 | `DNR_GUI_OK`，退出码 0 | `DNR_GUI_OK`，退出码 0 |
| `/tmp` 直接执行 CLI / GUI `.dnp`，参数与 cwd | 通过 | 通过 |
| 真实 Wayland / NVIDIA 页面与窗口 | 通过，保留协议日志 | 通过，保留协议日志 |
| 最后一个窗口关闭后，仅托盘保活；销毁托盘后退出 | `DNR_TRAY_OK`，退出码 0 | `DNR_TRAY_OK`，退出码 0 |
| GUI 中 Deno / Node 显式退出、未捕获异常 | 退出码依次为 7 / 9 / 1 | 退出码依次为 7 / 9 / 1 |
| KWin 原生关窗、原始 Songjian 包退出 | 通过，无需 Ctrl-C，细节见下文 | 通过，无需 Ctrl-C，细节见下文 |
| 两个 appId 并行、Deno localStorage 隔离与持久化、包重命名 | 通过 | 通过 |
| 测试后无遗留运行进程 | 通过 | 通过，包括 CEF 子进程 |

共同检查：`cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`、`cargo test --locked --workspace` 最终均退出 0；12 项包测试通过，普通测试命令中的 6 项原生测试按设计忽略，再分别指定两个后端显式运行并全部通过。原生测试包括真实 Node-API 插件：磁盘可加载，ZIP 内明确拒绝，包旁磁盘回退可加载。

上一轮 CEF 托盘测试另连续重复 5 次，均输出 `DNR_TRAY_WINDOW_CLOSED`、`DNR_TRAY_OK` 并退出 0，覆盖关窗时未完成 JS 求值的收尾。下述显式退出修复后的两个后端也重新通过托盘及全部补充测试。

### Songjian 手动关窗问题的补充回归

用户报告同一份 macOS 构建的 `~/下载/Songjian.dnp` 在 Linux 关窗后仍需 Ctrl-C。检查包内入口可见，应用已注册同步 `close` 监听器调用 `Deno.exit(0)`。旧 WebView 与 CEF 均复现：原生关闭事件已经送达 JS，但监听器调用退出后，HTTP 服务和定时器仍继续运行。此前的窗口测试使用 `win.close()`，没有覆盖这条原生关闭事件路径。

根因在上轮新增的宿主退出接管：仅在 worker 的异步执行完成后检查 `WatcherExited`。从微任务调用 V8 终止时，事件循环 future 仍可能因活动 HTTP 服务返回 `Pending`，宿主于是一直等不到退出。`integration/rt/desktop_tail.rs` 现在在执行 future 的每次轮询后检查退出标记，及时将请求的退出码交回主线程进行原生清理，保留没有显式退出时的后台任务与托盘生命周期。

- 新增 `explicit_exit_with_active_server` 原生测试：微任务中的 Deno / Node 显式退出必须终止仍监听的 HTTP 服务，分别返回 7 / 9；旧产物在 10 秒限制内未退出而失败，修复后的两个产物均通过。
- 新增 `examples/desktop/native-close.ts` 与 `scripts/test-linux-close.py`。脚本通过 KWin 向本次启动进程的窗口发送原生关闭请求，覆盖显式 Deno / Node 退出、HTTP 服务异步收尾和无服务窗口的后台任务，不使用 `win.close()` 或 Ctrl-C 作为成功路径。
- WebView 与 CEF 的四种模式及原始 Songjian 包均连续完成 3 轮，退出码符合预期。CEF 最终脚本等待 KWin 登记窗口，避免 data URL 加载先完成、窗口尚未映射时漏发关闭请求。
- 原始 Songjian 包保持未修改，SHA-256 为 `5420e87aa1581deed1654f2d166408cb235189ea41f2531b9661cd7e27036ac4`。WebView 另重复 10 次，宿主在原生关闭请求后约 0.05–0.15 秒退出 0；CEF 三次均约 0.07–0.08 秒退出 0。
- WebView 的系统 `WebKitWebProcess` 偶尔在宿主退出后继续清理约 4.5 秒，随后自行结束；因此测试分别记录宿主退出耗时和整个进程组结束耗时，给系统组件最多 15 秒自然退出。最初 3 秒的子进程检查曾报告超时，保留了诊断记录。CEF 测试中子进程随宿主结束。最终通过的测试没有发送终止信号，没有永久残留进程。
- 补充回归期间普通包测试曾出现一次 `Text file busy`；串行复验与随后普通并行复验均为 12 项通过，未修改包格式实现。

复验命令（需要真实 KDE Wayland 会话和 `qdbus6`）：

```sh
python3 scripts/test-linux-close.py --dnr dist/webview/dnr \
  --package "$HOME/下载/Songjian.dnp" --repeat 3 \
  --logs dist/validation-linux/manual-close/webview
python3 scripts/test-linux-close.py --dnr dist/system-cef/dnr \
  --package "$HOME/下载/Songjian.dnp" --repeat 3 \
  --logs dist/validation-linux/manual-close/system-cef
```

本次构建、6 项原生测试、程序主动关窗的完整补充回归和 KWin 关窗日志均位于 `dist/validation-linux/manual-close/`。最终 CEF 关窗摘要为 `system-cef-final-summary.log`；WebView 完整回归为 `webview-repeat-summary.log`，子进程耗时追加记录为 `webview-cleanup-timing.log`。两个后端的构建日志分别为 `build-webview.log`、`build-system-cef.log`，退出码均为 0。

### 构建与复验命令

以下命令均在项目根目录执行，退出码均为 0；GUI 显式退出示例的预期非零码另列于上表。

```sh
cargo run --locked -p xtask -- prepare --deno ../mirror/deno --laufey ../mirror/laufey
mkdir -p dist/webview dist/system-cef
CARGO_BUILD_JOBS=8 cargo run --locked -p xtask -- build
cp dist/dnr dist/dnc dist/webview/
CARGO_BUILD_JOBS=8 cargo run --locked -p xtask -- build --backend system-cef
cp dist/dnr dist/dnc dist/system-cef/
dist/system-cef/dnr --check-system-cef
DNR_BIN="$PWD/dist/webview/dnr" cargo test --locked -p dnr-package --test runtime -- --ignored
DNR_BIN="$PWD/dist/system-cef/dnr" cargo test --locked -p dnr-package --test runtime -- --ignored
# 本机补充验证脚本及输入保存在日志目录中。
python3 dist/validation-linux/run-extra.py webview
python3 dist/validation-linux/run-extra.py system-cef
```

### Linux 产物

| 文件 | 大小 | 内容 |
| --- | --- | --- |
| `dist/dnr`、`dist/webview/dnr` | 92,875,960 字节（88.57 MiB） | WebView release，ELF x86-64，已精简符号 |
| `dist/system-cef/dnr` | 94,695,144 字节（90.31 MiB） | system-CEF release，ELF x86-64，已精简符号 |
| `dist/dnc` | 2,381,312 字节（2.27 MiB） | Linux release 打包器，两个后端目录也各保留一份 |
| `dist/hello.dnp` | 783 字节 | CLI 示例 |
| `dist/desktop-smoke.dnp` | 4,473 字节 | 桌面示例目录，入口 `smoke.ts`，含原生关窗回归示例 |

本轮结束时 `dist/dnr` 已恢复为 WebView 版。两个 runtime 均无随附 `libdenort` / Laufey 动态库依赖；GTK/WebKitGTK 或系统 CEF 仍是外部依赖。

### 证据与边界

本机完整日志与补充验证脚本位于 `dist/validation-linux/`（生成目录，不纳入 Git），包括环境、构建、原生测试、Wayland 协议、退出码、崩溃诊断及产物 SHA-256。两份补丁均经过干净上游快照的 `prepare` 和反向 `git apply --check`，依赖锁文件未改变。

首次 Linux 验收的构建日志为 `build-webview-final.log`、`build-system-cef-eval-fixed.log`，测试见 `webview-runtime-final.log`、`system-cef-runtime-final.log`、`webview-extra-final.log`、`system-cef-extra-final.log`。CEF 求值回调重复回归见 `cef-tray-repeat-final.log`。Songjian 关窗修复后的最终日志见上述 `manual-close/` 子目录；根日志目录的 `webview-sha256.txt`、`system-cef-sha256.txt` 已更新为当前产物。早期失败日志保留用于追踪修复，不代表最终状态。

GUI 结果来自真实 KDE Wayland 会话中的原生后端自动验证，不是无头浏览器或仅检查 HTTP 监听。未进行独立 X11 会话、其他发行版/GPU、托盘菜单人工点击、浏览器页面 cookies/存储持久化的专项验收；存储隔离测试针对 Deno `localStorage`。本轮未复验 macOS。固定上游仍有弃用/未使用变量编译警告，Ayatana 托盘库会输出弃用提示。

### 系统安装与松间桌面接入（2026-09-18）

按用户要求，仅安装 system-CEF 运行时供松间使用：`/usr/local/bin/dnr` 与 `dist/system-cef/dnr` 校验值一致，`/usr/local/bin/dnc` 与 `dist/dnc` 一致。安装后 `dnr --check-system-cef`、`dnc --version` 均退出 0；开发目录的 `dist/dnr` 仍保留 WebView 版，系统 PATH 中的 `dnr` 为 system-CEF 版。

相邻 Songjian 项目经 Vite+ 构建后生成 48,210 字节的 `.dnp`。旧 `/opt/Songjian` 中的独立 `Songjian.so`、原生后端与 CEF 资源链接已移除，替换为约 104 KiB 的应用目录和共享运行时启动器。原桌面入口 ID `world.fansionia.songjian`、中文名称和主题图标沿用；启动器在每次运行前检查 system-CEF ABI。

使用 GIO 从实际安装的 `.desktop` 入口启动，KWin 报告窗口标题“松间”、`resourceClass` 与 `desktopFileName` 均为 `world.fansionia.songjian`，匹配安装的主题图标；发送原生关窗请求后退出码为 0。GUI 验证使用隔离数据目录，安装前后真实 `workspace.json` 的 SHA-256 一致。安装与桌面启动证据保存在 `dist/validation-linux/install/`，没有将应用数据加入仓库。
