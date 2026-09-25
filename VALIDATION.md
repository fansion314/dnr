# 验证记录

前半部分 macOS 记录为历史验收结果；Linux 记录及最新的 macOS 性能优化复验见本文后半部分。

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

## Songjian 应用接入（2026-09-18）

- 在 macOS ARM64 上使用现有 release dnc/dnr 重新打包 Songjian；前端由 Vite+ 单独构建，
  `Songjian/release/dnr/Songjian.dnp` 为 48,210 字节，包含前端资源和 4 个桌面源码/配置文件，不带运行时。
- Songjian 桌面入口改用随机本机端口并显式导航窗口；新增 `desktop:build:dnr` 命令。
  实际产物从 `/private/tmp` 启动成功，随后主动结束该验证进程。
- 使用相同前端和桌面代码、仅替换数据目录并注入测试探针的独立烟雾包，从 `/private/tmp` 直接执行两次。
  真实 WebView 自动确认 Svelte 渲染、120% 缩放、DOM 添加待办、JSON 落盘及新进程/新端口恢复。
  两次均输出 `SONGJIAN_GUI_OK`，程序关窗并关闭 HTTP 服务后退出码均为 0。
- 程序调用 `window.close()` 不触发用户关闭请求的 `close` 事件；测试需显式关闭 HTTP 服务。
  本轮未自动点击红色关闭按钮，未验证音频或 Linux，也未重新构建原 Deno Desktop `.app`。
- Songjian 的 `vp check`、`vp run check`、Deno 类型检查/lint 通过；29 项前端/脚本测试及
  4 项 Deno 桌面测试通过，5 项 Linux 安装测试在 Mac 上跳过。dnr 本身未修改实现或重建。

## Songjian macOS 薄应用安装（2026-09-18）

用户明确要求独立 macOS 应用及 Dock 图标，并选择继续共享 dnr。实现位于 Songjian 的
`scripts/macos-dnr.mjs`、`scripts/macos-launcher.m` 和 `scripts/install-macos-dnr.mjs`，
dnr 核心及运行时二进制未修改。

- ARM64 原生启动器 + Info.plist + ICNS + `.dnp` 的 `.app` 实际文件总大小 182,314 字节，
  安装到 `/Applications/松间.app`；共享 dnr 为 74,095,072 字节。
- 共享运行时安装在当前用户 `~/Library/Application Support/dnr/runtimes/<SHA-256>/dnr`，
  应用按哈希引用固定版本；已核对安装后哈希与构建输入一致，不引用源码仓库路径。
- `codesign --verify --deep --strict` 通过。LaunchServices 实测显示名称“松间”、
  Bundle ID `world.fansionia.songjian`、bundle path `/Applications/松间.app`，
  executable path 为共享运行时，originalExecutablePath 为包内原生启动器。
- 原图背景铺满画布导致 Dock 图标视觉过大；macOS 派生 SVG 改为 1024 画布内 824 主体，
  每边 100 透明像素，resvg 渲染各尺寸后生成 ICNS。用户已观察修正后的测试版并确认“现在大小正常”。
  自动 UI 工具超时，未取得自动截图；该视觉结论来自用户确认。
- 安装时曾备份旧版 `.app`，确认替换成功后按用户要求删除了本次备份。
  替换完成后从正式安装位置启动成功；安装前后 `workspace.json` 哈希相同。
- Songjian 的格式/lint/类型检查、Svelte 检查和 29 项现有测试通过；5 项 Linux 专用测试跳过。
  Objective-C 启动器以 `-Wall -Wextra -Werror` 编译通过。应用为本地 ad-hoc 签名，未公证。

## 原有范围

此前延后的 Linux 验收已于 2026-09-18 在用户指定的本机继续执行，结果如下。复验步骤见 [LINUX.md](docs/LINUX.md)。

截至上述历史验证，dnr/dnc 核心尚不包含 ZIP 原生库释放和通用桌面打包；后续已增加按需临时解压、macOS 薄应用和 Arch/CachyOS 打包，见文末相应记录。仍不包含 npm/JSR/HTTP 模块在线安装、DMG/PKG 安装器、深链注册或自动更新。
上述历史 Songjian 薄应用由其应用项目单独构建和安装。


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

## macOS 隐藏与 Dock 恢复（2026-09-18）

本轮在 macOS 27.0（26A428）ARM64 / Rust 1.98.1 上，将远程 `b81c530`
快进合并到本地，并保留原有 Songjian 文档改动。随后修复 WebView 后端的两个问题：

- 默认应用菜单缺少 `hide:` 菜单项，现增加 Cmd+H，并将目标显式设为 `NSApp`。
  应用自行替换菜单时仍由自定义菜单负责提供 `hide` role。
- `applicationShouldHandleReopen:hasVisibleWindows:` 原先总是返回 `NO`，拦截系统恢复窗口。
  现保留 JS `Dock.reopen` 通知并返回 `YES`，允许 AppKit 执行默认恢复；同步更新接口注释。
  返回值语义见 [Apple 文档](https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationshouldhandlereopen(_:hasvisiblewindows:))。

### 实际验证

- 旧版二进制的真实窗口上，Cmd+H 后应用仍可见；点击黄色最小化按钮再点击 Dock 后，
  `AXMinimized` 仍为 `true`。新增脚本对旧版执行时明确失败于 `Cmd+H did not hide application`。
- 新版 `python3 scripts/test-macos-window.py dist/dnr` 退出 0，输出 `DNR_MACOS_WINDOW_OK`。
  使用 macOS System Events 实际发送 Cmd+H、点击黄色按钮及 Dock 图标，连续两轮确认：
  应用隐藏、Dock 取消隐藏并激活、最小化、Dock 还原、没有重复窗口。
  同时收到 4 次 JS Dock reopen 通知；点击原生关闭按钮后异步收尾完成，进程退出 0。
- 两份补丁的所有目标文件先恢复并逐字核对固定上游版本，再执行 `xtask prepare`，成功应用。
  `xtask build` release 成功；sccache 保持启用。构建时 Cargo 缓存了先前的 sccache
  权限失败，重启 sccache 并使用 `CARGO_CACHE_RUSTC_INFO=0` 重新探测后恢复。
- `cargo test --workspace`：12 项包测试通过；显式执行新版 release 的原生测试，6 项全部通过。
  `cargo clippy --workspace --all-targets -- -D warnings` 和 `cargo fmt --all -- --check` 通过。
- 重新打包 `examples/desktop/smoke.ts`，从 `/private/tmp` 启动应用包，输出 `DNR_GUI_OK`、
  退出 0，覆盖页面绑定及关闭后的异步任务。`codesign --verify --strict --verbose dist/dnr` 通过。

新版产物为 `dist/dnr`，SHA-256：
`e91a63c9688a2a9ae439d7ce661ac2110a9cb0568a2a672afcf843d90b0b3f52`。
本轮没有替换已安装的 Songjian `.app` 或其按哈希固定的共享运行时；应用需更新运行时引用后
才会使用本次修复。没有在本轮重新执行 Linux GUI 验证，Linux 证据仍来自合并的远程记录。


## 性能优化复验（2026-09-18，macOS ARM64）

基线为 `a65f01e` 的原生 release，本轮最终源码使用相同 Rust 1.98.1、固定上游和构建配置。
保持 sccache 启用；最初 Cargo 缓存了沙箱中的编译器探测权限错误，提权并设置
`CARGO_CACHE_RUSTC_INFO=0` 重新探测后构建成功，没有关闭 sccache。

### 性能测量

新增 `crates/package/examples/performance.rs`。每项预热一次，取 7 次样本中位数；
最终运行时的基准在编译结束后独立运行，并用保留的旧版二进制再次核对端到端基线。
详细负载、计时边界、代码优化和保留成本见 [PERFORMANCE.md](docs/PERFORMANCE.md)。

| 合成负载 | 优化前 | 优化后 | 比值（前 / 后） |
| --- | --- | --- | --- |
| 2,500 个模拟 npm 目录、10,000 个小文件的应用启动 | 1,141.886 ms | 71.544 ms | 约 16.0 倍 |
| 10,000 个驻留文件，20,000 次缓存命中 | 123.048 ms | 1.244 ms | 约 98.9 倍 |
| 100 个驻留文件，20,000 次缓存命中 | 1.872 ms | 1.224 ms | 约 1.5 倍 |
| 10,000 ZIP + 5,000 磁盘文件（2,500 同名），40 次目录合并枚举 | 8,037.182 ms | 303.217 ms | 约 26.5 倍 |

缓存微基准的旧值在修改包实现前测得；两次端到端对照均直接执行对应版本的原生 dnr。
这些是暖文件系统缓存下的合成负载，不是冷启动、真实应用普遍加速比例或 Linux 性能证据。
`Package::open` 本身约 30–33 ms，没有声称这一未改动路径得到加速。
读取缓冲的试验没有测出收益，最终实现已撤回该试验。

### 最终验证

- `cargo fmt --all -- --check` 与 `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo test --workspace`：14 项包测试通过；原生测试仍默认忽略。
- 最终精简并签名的 `dist/dnr` 上显式执行原生测试：7 项全部通过，退出码 0。
  新增大目录树、相近前缀目录、Unicode、空目录、符号链接和 ZIP/磁盘目录类型与同名覆盖回归。
- LRU 参考模型测试覆盖 5 种预算 × 2,000 次访问；CRC 损坏仍懒读取报错，失败数据不缓存。
- 最终 dnc 重新生成 `dist/desktop-smoke.dnp`，从 `/private/tmp` 直接启动；真实 WebView
  页面与双向绑定成功，关窗后异步任务完成，输出 `DNR_GUI_OK`，退出码 0。
- `codesign --verify --verbose dist/dnr` 通过；`otool -L` 仍仅显示系统库和 Framework。
- 更新后的 Deno 补丁在干净的固定版本文件副本上通过应用检查，结果逐文件等于构建树；
  最终反向检查通过，两份 mirror 工作区保持干净。
- 已更新 `dist/dnr`、`dist/dnc` 与产物校验和。本轮未覆盖用户已安装的共享运行时。
- 本轮没有执行 Linux WebView/system-CEF 构建或 GUI 复验，也未重跑原生标题栏关闭等
  平台专项验收；此前记录保留为历史证据。桌面生命周期代码未作修改。


## 整文件读取复制优化（2026-09-18，macOS ARM64）

基线为 `4467e78` 的 release。同步/异步 `op_fs_read_file_*` 将
`buf.into_owned().to_vec().into()` 改为 `buf.into_owned().into()`，由返回的
Uint8Array 接管独占缓冲区；借用数据仍由 `into_owned()` 复制。
改动保存在 `integration/deno.patch`，不修改 mirror、缓存策略、模块加载或桌面生命周期。

- `cargo run -p xtask -- build`：macOS ARM64 release 构建成功，sccache 保持启用。
- `cargo test --workspace`：14 项包测试通过；8 项原生测试默认忽略。
- 最终 release 显式执行原生测试：8 项全部通过。新增返回值所有权测试先在旧版通过，
  再在新版验证 Deno 同步/异步、Node 同步/Promise/回调 API 的内容与可独立修改性。
  覆盖空文件、4,097 字节及 2 MiB 文件、同时发起的读取、磁盘入口、ZIP 与磁盘回退。
- 格式检查、Clippy 与补丁应用检查通过；完整补丁作用于干净固定版本文件后的结果逐文件等于构建树。
- 最终 release 从 `/private/tmp` 运行桌面烟雾包，输出 `DNR_GUI_OK`，退出码 0；签名验证通过。
- 本轮未执行 Linux 原生构建/验证，也未更新用户已安装的 dnr/dnc。

新增可复用基准 `scripts/bench-read-file.py`，在编译结束后执行：

```sh
python3 scripts/bench-read-file.py --dnr target/read-copy-baseline/dnr dist/dnr
```

每个样本使用独立进程，先预热整个 16 MiB ZIP 条目，再计时 8 次读取；
丢弃一轮预热结果，取后续 7 轮中位数，逐轮交替新旧二进制顺序。
计时不含进程启动或首次解压，包含返回值分配；同样校验长度和抽样内容。

| 每次 16 MiB 热缓存读取 | 优化前 | 优化后 | 耗时减少 |
| --- | --- | --- | --- |
| `Deno.readFileSync` | 1.6956 ms | 1.0869 ms | 35.9% |
| `Deno.readFile` | 1.6728 ms | 1.0748 ms | 35.7% |

这是本机合成负载，约 1.56 倍吞吐改善，不代表所有文件大小或应用同比提速。
源码可确定减少一次整文件复制；约 3S → 2S 的瞬时缓冲区存活量是大小模型，
本轮没有把它当成实测 RSS 降幅。返回后的缓存及 JS 数组占用、GC 压力和首次解压成本仍存在。


## 并行 ZIP 解压与 CLOCK 缓存（2026-09-18，macOS ARM64）

环境：Apple M3 / 8 核 / 16 GiB，macOS 27.0（26A428），Rust 1.98.1；固定 Deno/Laufey
与依赖锁未变。保留 sccache；首次沙箱测试因 sccache 权限失败，提权后设置
`CARGO_CACHE_RUSTC_INFO=0` 重新探测并成功构建，没有关闭 wrapper。

### 实现与测量

- 位置读取器共享同一个已打开文件，但各自保留逻辑游标；zip 8.6.0 的 archive 克隆共享中央目录。
  不同条目独立解码，同条目通过 Flight 合并正在执行的读取，包括未缓存的大文件和失败结果。
- 缓存改为每条目短锁与按字节预算的 CLOCK；命中不获取全局管理锁。保留 CRC/尺寸/EOF 校验、
  只在不存在时回退磁盘、懒解压与文件句柄 Arc 保活。新增代码不含 unsafe、不增加依赖。
- 每包并发最多 min(可用并行度, 8)，在途声明解压尺寸合计默认不超过 256 MiB；更大文件单独执行。
  这是解码准入限制，不是进程 RSS 上限；驻留缓存仍有独立的 256 MiB 默认预算。
- 编译和测试结束后运行相同输入、相同 release 配置的前后对照。真实 Worker：16 × 8 MiB
  资源，冷缓存单 Worker 为 173.611 → 171.617 ms；2/4/8 Worker 分别为
  172.854 → 89.614、173.930 → 52.139、175.447 → 49.382 ms，8 Worker 约快 3.55 倍。
- 真实 8 Worker 热缓存文件 API：20,000 次打开/首尾读取/关闭，39.520 → 34.834 ms；
  Rust 包层 8 线程的 200,000 次热命中为 41.188 → 2.027 ms。上层 API 成本仍在，
  不把微基准收益等同于整个应用收益。完整表格及计时边界见 `docs/PERFORMANCE.md`。
- 每项预热一轮，取 7 轮中位数；Worker 对照逐轮交换二进制顺序。冷缓存指解压缓存，
  操作系统文件缓存是暖的。单线程冷解压没有稳定加速结论，本轮没有测量进程峰值 RSS。

### 验证

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `git diff --check` 通过。
- `cargo test --workspace`：5 项包层单元测试与 16 项包集成测试通过；9 项原生测试按设计忽略。
  确定性测试确认两个不同条目在任一任务完成前均能进入解码，且驻留命中可同时完成；
  同条目等待者共享结果/错误，panic 展开唤醒后可重试，超预算数据也合并本轮读取。
  另覆盖 CLOCK 第二次机会、5 种预算 × 2,000 次访问、并发淘汰及保留引用、
  在途数量/字节限制、CRC 损坏不缓存及修复后重试、包路径替换与非法索引。
- `DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored`：
  最终 release 上 9 项全部通过，退出 0。新增 8 个真实 Worker 同时读取相同与不同 ZIP 文件，
  校验全部字节及 seek/EOF；原有 TS/CJS、Node 插件磁盘边界、目录合并、读结果所有权、
  HTTP 服务与显式退出回归全部通过。
- release `xtask build` 成功，输出已精简并 ad-hoc 签名的 `dist/dnr` 与 `dist/dnc`。
  `codesign --verify --strict --verbose dist/dnr` 通过；`otool -L` 仍仅显示系统库与 Framework。

- 新版 dnc 生成桌面烟雾包，从 `/private/tmp` 直接启动，真实 WebView 页面与双向绑定通过，
  关窗后异步收尾完成，输出 `DNR_GUI_OK` 并退出 0；日志为 `gui-smoke.log`。

原生基线为本轮开始前的 release，SHA-256：
`3feb4b987e4c212ee8b3424384e1077f3aea7cca0deef1f414cc96747e7775fd`。
新版 `dist/dnr` SHA-256：
`b26a98d748f5ba4a8eab032e09c0bbb85741762bfdb2a937c6ddcebed53ad71c`。
包层基线使用 `4467e78` 的 `lib.rs` 与同一个新基准程序，临时源码位置记录在证据目录。

构建、工作区测试、原生测试、Worker 原始测量、包层前后对照和基准包保存在
`dist/validation-parallel-cache/`。保留本轮开始前的未提交整文件读取优化及文档；
本轮没有修改 Deno/Laufey 补丁或锁文件，没有安装/覆盖用户已安装的共享运行时；构建验证时尚未提交，也未推送。
本轮未执行 Linux WebView/system-CEF 原生构建或 GUI 验证，也未重跑平台专用的标题栏关闭与 Dock 回归。


## ZIP 原生插件统一临时解压（2026-09-18）

按用户最终确定的方案，ZIP 内原生库统一释放到系统临时目录，不检查、复用或写入包旁同名文件。
本节替代历史记录中“ZIP 内原生库拒绝加载”的能力边界；历史验收结果本身不改写。

### 实现与范围

- Node-API / FFI 加载钩子通过 `Package::native_library_path` 解析包内符号链接，
  完整校验解压尺寸和 CRC，再将请求的库原子发布到 `dnr-native-*` 私有目录（Unix `0700`）。
  保留包内相对路径及文件名，不修改 VFS 内容，不全量释放应用。
- 同一 Package 的主线程和 Worker 共享已解压路径；独立进程 / Package 使用不同临时目录。
  即使包旁存在另一版本，也加载 ZIP 内的版本。只有 ZIP 不包含的库才继续走原有磁盘加载。
- 宿主处理正常结束、Deno / Node 显式退出和 JS 异常时清理临时目录；独立 Package 对象销毁也会清理。
  强制杀进程、原生崩溃或应用自行修改临时目录权限时不保证清理成功。
- 原生插件仍需兼容当前平台、架构及 Deno 的 Node-API；不自动收集其 OS 动态链接依赖。
  临时目录不可写或不能加载动态库时返回错误，不回退到包旁副本。

### 验证

本机 macOS 27.0（26A428）ARM64、Rust 1.98.1、Apple Clang 21.0.0，WebView 后端。
sccache 保持启用；沙箱内编译权限失败后通过提权执行完成构建和测试。

- 从固定 mirror 的干净 Deno 快照执行 `cargo run -p xtask -- prepare` 成功，
  新补丁的反向 `git apply --check` 通过；Deno / Laufey mirror 工作区保持干净。
- `cargo test --workspace`：5 项单元测试、16 项包测试、3 项原生解压包层测试全部通过（24 项）；
  13 项需真实宿主的测试按设计忽略，再由下述命令显式执行。
  包层覆盖共享路径、独立临时目录、退出清理、包内符号链接、磁盘副本不覆盖 ZIP、
  目录权限，以及 CRC 损坏时不读取磁盘副本。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、
  接入源码的 `rustfmt --check`、`git diff --check` 均通过。
- 最终 release `xtask build` 成功。以下命令退出 0，9 项原有运行时测试与 4 项新增原生测试全部通过：

  ```sh
  DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime --test runtime_native -- --ignored
  ```

  测试编译并加载真实 Node-API 插件，由插件通过 `dladdr` 返回实际 OS 加载路径。
  确认 6 个并行进程的临时路径独立，8 个 Worker 与主线程共享路径，重复 require 可用，
  实际临时目录权限为 `0700`；只读包目录可以运行，包旁不同版本不影响 ZIP 内版本。
  正常结束、Deno 显式退出 7、Node 显式退出 9、JS 异常退出 1 后临时目录均清空。
  不可用 TMPDIR 明确报错；真实 `Deno.dlopen()` 加载 ZIP 内 `.so` 并调用导出函数成功。
  磁盘脚本和 ZIP 缺少插件时的磁盘加载仍通过原有回归。
- `codesign --verify --strict --verbose dist/dnr` 通过；`otool -L` 仍仅包含系统库和 Framework。

- 使用最终 dnc 生成桌面烟雾包，从 `/private/tmp` 用最终 dnr 启动。真实 WebView 页面与绑定检查通过，
  关窗后异步收尾完成，输出 `DNR_GUI_OK` 并退出 0；日志为 `dist/validation-native-tmp/gui-smoke.log`。

最终 `dist/dnr` SHA-256：`5aab8dcf9fc4b1f88e0c182cf21fdd8dd6472bf4399281bd9738fa9e6b52bca8`。
工作区及原生测试日志保存于 `dist/validation-native-tmp/`。
本轮未执行 Linux WebView / system-CEF 原生复验；未修改用户已安装的共享运行时，未提交或推送。

## 包目录树与完整解压（2026-09-18）

新增 `dnr tree <application.dnp>` 和 `dnr extract <application.dnp> <directory>`。
命令在进入 JS/GUI 宿主之前处理，包含 ZIP 内的原始 manifest，不使用运行时的磁盘回退。
解压流式校验内容，在同级私有临时目录准备完成后发布至新目录或空目录；非空目录、文件和符号链接目标均拒绝覆盖。

本机 `Darwin arm64`、Rust 1.98.1、WebView 后端，sccache 保持启用（构建/测试通过沙箱提权运行）。

- `cargo test --locked --workspace`：29 项通过，14 项真实宿主测试按设计忽略。
  新增 5 项包层测试覆盖目录排序、隐式父目录、Unicode/控制字符显示、链接不展开、原始 manifest 字节、
  Zstd 文件、空文件/空目录、执行权限、新建/现有空目录、非空目录与符号链接目标拒绝覆盖。
  CRC 损坏时仍可查看树，但解压失败不发布部分目标内容；越界路径、链接循环、文件/目录冲突和规范化重名均拒绝。
- `cargo clippy --locked --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、
  `rustfmt --check --edition 2024 integration/rt/dnr_main.rs`、`git diff --check` 均通过。
- `cargo run -p xtask -- build`：macOS ARM64 WebView release 构建成功，更新 `dist/dnr` 和 `dist/dnc`。
  `codesign --verify --strict --verbose dist/dnr` 通过；真实执行 `dist/dnr tree dist/hello.dnp` 显示
  `.dnr/manifest.json`、`main.ts` 与 `message.txt`。
- `DNR_BIN="$PWD/dist/dnr" cargo test --locked -p dnr-package --test runtime --test runtime_native -- --ignored`：
  10 项 runtime 测试与 4 项 Node-API/FFI 原生测试全部通过，退出码 0。
  新增命令回归使用入口必定抛错的包，确认 tree/extract 不执行入口，路径含空格可用，
  缺失/多余参数及非空目标报错，帮助可用，脚本参数透传及 `dnr ./tree` 仍可正常运行。

本轮未执行 Linux WebView / system-CEF 原生验证或真实 GUI 复验；未修改已安装的共享运行时。


## dnc desktop manifest 打包（2026-09-18，macOS ARM64）

新增 `--desktop-manifest` / `--target macos|archlinux`，保留原 `.dnp` 命令。
实现仅修改 dnc 及文档，未修改运行时、包格式或 Songjian 源码；未安装、替换已有应用。
macOS 原生启动器参考 Songjian 薄应用，Linux 输出由系统 makepkg 生成。
配置与安装方式见 [DESKTOP-PACKAGING.md](docs/DESKTOP-PACKAGING.md)。

### 已执行

- `cargo test --workspace`：新增 3 项 dnc 单元测试与 1 项 CLI 回归、原有 14 项包测试通过。
  原有 7 项 runtime 测试和新增 macOS 原生打包测试在普通命令中显式忽略。
- 单元测试执行了生成的 Linux shell 启动器和 PKGBUILD 的文件复制函数，覆盖
  WebView/system-CEF、中文/引号/反斜杠与 shell 插值字符、空参数、应用身份、
  CEF 检查失败退出码、配置错误以及输出替换失败时旧文件保留。
  这不是原生 Linux makepkg/pacman 验收。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、
  `git diff --check` 通过。sccache 保持启用，初始沙箱权限错误后提权执行 Cargo。
- 显式运行 `macos_bundle_real_runtime`，分别使用 Songjian 的 ICNS 与 PNG 图标，
  真实编译 ARM64 原生 launcher、转换 PNG 图标、签名并执行验证，测试全部通过。
  覆盖中文/空格输出目录、首次打包、拒绝无 force 覆盖、force 重建不递归包含旧 `.app`，
  以及真实 dnr 的参数、cwd、应用名和 appId。最后的权限归一化改动后重新通过 PNG 路径。
- 使用现有 `/Users/peilin/Codebase/dnr/dist/dnr`，未重建 runtime。
  将 `examples/desktop/smoke.ts` 打包到 `dist/validation-dnc/DNC-Smoke.app`，
  经 macOS `open -n -W` / LaunchServices 启动真实 WebView，自动验证页面及双向绑定，
  stdout 输出 `DNR_GUI_OK`，应用自动退出，open 返回 0。
  构建自动完成 `codesign --verify --deep --strict`。
- `cargo build --release -p dnc` 成功；新版工具已复制到本工作区 `dist/dnc`，`--help` 显示新选项。
- GUI 日志保存在 `dist/validation-dnc/gui.stdout.log`、`gui.stderr.log`。
  测试使用的 manifest 指向当前机器的现有 dnr 和图标，属于本地验证配置，不是可分发配置。

### 未执行的范围

- 本机没有原生 Arch/CachyOS 环境，未执行完整 makepkg、pacman 查询、安装/升级/卸载或
  Linux 菜单启动和窗口图标验证。已提供需显式运行的 `arch_package_metadata_and_payload`
  测试，检查真实包元数据及 `.PKGINFO`、`.BUILDINFO`、`.MTREE` 和安装路径；
  应在原生 Arch/CachyOS 普通用户环境运行，命令见打包文档。
- 未测试 macOS Developer ID 签名、公证、应用商店分发或最低支持系统版本；
  当前产物只有本地 ad-hoc 签名。未替换 `/Applications` 中已安装的 Songjian 或共享运行时。
- 没有重新执行底层 runtime 原生回归；本轮测试复用现有已构建 dnr。

## 桌面打包与包操作命令合并复验（2026-09-18，macOS ARM64）

将桌面打包提交 `a8fb86f` 与当前 main 的 runtime 优化、原生插件临时解压及
tree/extract 命令合并；冲突仅涉及文档，保留两侧功能说明和各自历史验证范围。

- `cargo test --locked --workspace`：33 项通过，15 项原生测试按设计忽略。
- `cargo clippy --locked --workspace --all-targets -- -D warnings`、
  `cargo fmt --all -- --check`、`git diff --check` 全部通过。
- `cargo run -p xtask -- build` 成功，生成最终 macOS ARM64 WebView release dnr/dnc，
  sccache 保持启用。`codesign --verify --strict --verbose dist/dnr` 通过。
- 设置 `DNR_BIN` 为本工作区 `dist/dnr`、`DNC_TEST_ICON` 为 Songjian 的 PNG 图标后，
  `cargo test --locked --workspace -- --ignored`：15 项全部通过。
  包括 macOS 原生启动器编译、PNG 转 ICNS、签名、首次与 force 打包、参数/cwd/身份，
  以及 10 项 runtime、4 项 Node-API/FFI 测试。
- 本次未执行真实 GUI 或 Linux 原生复验；历史 GUI 与 Linux 证据仍以各节范围为准。

## macOS 更新后的 Linux 复验与应用安装（2026-09-18）

本轮以 `6d4062b` 为源码基线，在 CachyOS x86_64 / Linux 7.2.6-1-cachyos-bore-lto、
KDE KWin 6.7.5 Wayland、NVIDIA RTX 3080 / 615.71.09 上原生执行。
Rust 1.98.1、GCC 16.2.1、CMake 4.4.3；GTK 3.24.52、WebKitGTK 2.52.6、
系统 CEF 152.0.6-1，CEF API 14900。没有使用交叉构建或无头浏览器替代原生验收。

### 构建与新功能

- 保留旧缓存目录后，从固定 mirror 提交重新 `xtask prepare`，两份补丁干净应用成功，
  构建后反向 `git apply --check` 通过；mirror 工作区未修改。
- sccache 保持启用。首次沙箱内编译器探测报权限错误，改为沙箱外执行并设置
  `CARGO_CACHE_RUSTC_INFO=0` 后正常完成，没有禁用 wrapper。
- `cargo test --locked --workspace`：33 项通过，15 项宿主/平台测试按设计忽略。
  `cargo clippy --locked --workspace --all-targets -- -D warnings`、格式检查通过。
- 原生 `arch_package_metadata_and_payload` 显式测试通过：真实调用 makepkg，验证
  pacman 元数据、压缩包结构和安装文件。命令与输出见 `arch-package.log`。
- 分别执行 `xtask build` 和 `xtask build --backend system-cef`，两个 release 构建成功，
  保存在 `dist/webview/`、`dist/system-cef/`。本轮结束时 `dist/dnr` 为 system-CEF 版。
- 两个后端分别显式执行 `DNR_BIN=... cargo test --locked -p dnr-package --test runtime
  --test runtime_native -- --ignored`：每个后端 10 项 runtime、4 项原生测试全部通过。
  覆盖并行 Worker / CLOCK 缓存语义、文件读取所有权、目录合并、tree/extract、
  ZIP 内 Node-API / FFI、只读包目录、主线程与 Worker 复用临时路径、多进程隔离，
  以及正常退出、显式退出和异常退出后的临时目录清理。
- 两个 ELF x86-64 产物都无缺失动态库、随附 libdenort 或 Laufey 动态库依赖。
  system-CEF 的 `libcef.so` 来自 `/usr/lib/cef`，`--check-system-cef` 通过。

### 真实桌面回归

- 两个后端均通过磁盘与 ZIP GUI 烟雾测试：真实页面、双向绑定、关窗后的异步收尾，
  输出 `DNR_GUI_OK` 且退出 0。ZIP 从 `/tmp` 执行，另验证 CLI 参数与调用者 cwd。
- 两个后端均通过托盘保活、多应用独立进程、appId 存储隔离与持久化，以及 GUI 中的
  Deno 退出 7、Node 退出 9、异常退出 1；最后确认测试 runtime 没有残留进程。
- `scripts/test-linux-close.py --repeat 2` 对每个后端执行两轮真实 KWin 原生关闭，
  覆盖 Deno、Node、异步关闭和自然结束四种模式，均通过；检查对应进程组全部结束。
  新 Songjian 包额外执行两轮 CEF 原生关闭，均退出 0，无遗留子进程。
- 首次 WebView 附加测试的全局进程扫描撞上同时运行的 pi 烟雾进程；pi 完成后单独重跑，
  `webview-extra-final.log` 全部通过。早期日志保留，不把测试间干扰记为 runtime 缺陷。

### Songjian 新包与旧安装迁移

- 相邻项目依赖按锁文件同步，执行
  `DNC=/home/fansion/codebase/dnr/target/release/dnc node scripts/dnr.mjs --target archlinux`。
  构建前端、Deno bundle，再由新 dnc / desktop manifest 调用 makepkg。
- 产物：`../Songjian/release/dnr-linux-x86_64/songjian-1.1.0-1-x86_64.pkg.tar.zst`，
  88,579 字节，包声明依赖 `cef`、`gtk3`，共享 dnr 为实际运行必需。
  安装前已向用户列出所有有效载荷和 `.PKGINFO`、`.BUILDINFO`、`.MTREE`。
  有效载荷只有以下四个文件：
  - `/usr/bin/songjian`
  - `/usr/lib/world.fansionia.songjian/application.dnp`
  - `/usr/share/applications/world.fansionia.songjian.desktop`
  - `/usr/share/pixmaps/world.fansionia.songjian.png`
- 真实执行新版 `dnr tree` 与 `dnr extract`，确认内部包含原始 manifest、
  `desktop/main.js`、前端 HTML/JS/CSS 与 favicon；没有捆绑 runtime 或 CEF。
- Songjian 前端类型检查无错误/警告，Deno 资源与存储测试 4 项通过，Vite+ Linux 脚本
  测试 13 项通过、5 项旧布局测试条件跳过。首次误用 Node runner 的日志保留；
  正确的 `vp test --run ...` 结果见 `songjian-vitest.log`。
- 经系统管理员认证，更新 `/usr/local/bin/dnr` 为本轮 system-CEF release，并更新 dnc。
  删除旧 `/opt/Songjian`、`/usr/local/bin/Songjian`、旧桌面入口与 hicolor 图标后，
  用 `pacman -U` 安装新包。宿主上的 `pacman -Qkk songjian` 为 11 项、0 项改变。
- 从实际安装的 `.desktop` 通过 GIO 启动；KWin 确认窗口标题“松间”，
  resourceClass / desktopFileName 为 `world.fansionia.songjian`，图标名匹配。
  原生关窗后退出 0。测试使用隔离数据目录；真实 `workspace.json` 安装前后 SHA-256 一致。

### pi 单文件构建与用户安装

- 相邻 pi 项目执行 `npm ci --ignore-scripts`、`npm run hydrate:model-data`，再执行
  `node scripts/build-dnp.mjs --dnc /home/fansion/codebase/dnr/target/release/dnc`。
  产物 `../pi/packages/coding-agent/dist/dnr/pi.dnp` 为 6,435,579 字节，版本 0.85.1，
  574 个应用文件，包含 Linux x64 原生插件、JS、WASM、主题、文档与许可证。
- 在 WebView、system-CEF 两个新版 runtime 上分别执行 `scripts/dnp-smoke.test.mjs`，
  均通过：离开源码目录单文件部署、原生插件释放与清理、CLI、TS 扩展、模型目录、
  Photon 图片缩放、bash 工具、会话保存、HTML 导出和调用者 cwd。
- 使用真实 PTY（本机无 tmux）验证交互输入、本地 faux 模型调用 bash 并回复，
  出现 `PI_TUI_TOOL_OK`、`PI_TUI_REPLY_OK`，Ctrl-D 退出 0。安装后使用系统 CEF runtime
  再次通过相同交互回归；没有调用付费模型 API。
- 用 `pacman -R pi-coding-agent` 删除旧系统安装，再安装单文件到
  `/home/fansion/.local/bin/pi`，所有者 fansion、权限 0755。通过 fish 的通用变量
  `fish_add_path -U /home/fansion/.local/bin` 持久加入 PATH。
  新 fish 中 `command -v pi` 指向该文件，`pi --version` 输出 `0.85.1`。
  旧 `/usr/bin/pi` 与旧 pacman 包均已移除；没有删除 pi 用户配置或会话。

### 证据与边界

日志、安装脚本、文件清单和校验和位于 `dist/validation-linux-update/`。
关键日志：`workspace.log`、`arch-package.log`、`webview-runtime.log`、
`system-cef-runtime.log`、`webview-extra-final.log`、`system-cef-extra.log`、
两个 `*-close.log`、`pi-smoke-*.log`、`pi-installed-interactive.log`、
`install.log`、`installed-desktop.log`。安装后 dnr、dnc、pi 均与验证产物逐字节一致。

| 产物 | SHA-256 |
| --- | --- |
| WebView dnr（92,929,208 字节） | `670cc21e58cdae5e3885eb2f788ea2a51d813bf51f06a52865306957cd831db6` |
| system-CEF dnr（94,744,296 字节） | `87e2511a156f54dc3fbc6b67f9133d7ea81df52ea293de17e5f4beda4f279a58` |
| dnc（2,707,024 字节） | `208754d9c5eb4d30277b7b2bd79904a67c6af7d2ae24d9e9ac99641a148c63a2` |
| Songjian pkg.tar.zst | `08953f6b71115c6bd57bc454110989347ce05a29b9c93296018db22f2351e9a4` |
| pi.dnp / 用户本地 pi | `6da005165bd846099ed8a72389e8a6471c610b373fd7f48128d25c49b6645422` |

本轮没有发现需要修改实现的新 Linux 问题。未重新测量性能加速比例，未执行 macOS、
独立 X11 会话或其他 GPU/发行版验证；真实剪贴板读写、付费模型请求和 Songjian 的全部
业务交互不属于本轮覆盖范围。没有提交或推送代码，两个应用源码工作区保持干净。

### pi 安装位置调整（2026-09-18）

按用户后续要求，将同一份已验证的 pi 从 `/home/fansion/.local/bin/pi` 迁移到
`/usr/local/bin/pi`，与 dnr 同目录，所有者 root、权限 0755。逐字节校验后删除旧位置文件；
SHA-256 未变。fish 中 `command -v pi` 为 `/usr/local/bin/pi`，`pi --version` 输出 `0.85.1`。

### KDE 启动器旧路径缓存修复（2026-09-18）

用户报告 KDE 启动器仍尝试 `/opt/Songjian/Songjian`。系统桌面文件已经正确指向
`/usr/bin/songjian`，没有找到用户级覆盖入口，但 `ksycoca6_en-POSIX_*` 缓存仍含旧路径；
中文缓存已含新路径。此前 GIO 启动验证没有覆盖 KDE 的服务缓存与启动流程。

以桌面用户分别在 C 与 `zh_CN.UTF-8` 语言环境执行 `kbuildsycoca6 --noincremental`。
重建后两份缓存均不再包含旧路径；直接调用 KDE `KService::serviceByDesktopName`
确认两种语言都返回已安装的桌面文件和 `/usr/bin/songjian`。
随后通过该 KService 与 `KIO::ApplicationLauncherJob` 真实启动，KWin 确认标题“松间”、
应用身份 `world.fansionia.songjian`，原生关闭后宿主进程消失，测试退出 0。
日志见 `dist/validation-linux-update/kde-cache-*.log`、`kde-launch.log`。
打包文档补充桌面用户刷新 KDE 服务缓存的步骤；不重建应用，不恢复旧 `/opt` 启动链接。


## v0.1.0 AUR 配方与发布准备（2026-09-18）

- 从 origin 快进合并 `d32963f` 的项目元数据、双语 README 与 MIT 许可证；保留并提交
  本地 Linux 复验和安装记录，同步 README 中已过时的验证状态。
- 新增 `packaging/aur/dnr`（system-CEF）与 `dnr-webview`（WebKitGTK）两份配方及
  `.SRCINFO`。源码来自 GitHub `fansion314/dnr` 的 `v0.1.0` 标签，上游使用完整固定提交。
  两包互斥，均安装 dnr/dnc；WebView 包提供 `dnr=0.1.0`。
- 依据 ELF 的 NEEDED、pkg-config、pacman 文件归属和后端源码核对直接链接依赖，
  补充托盘 `libayatana-appindicator`、通知 `libnotify` 和 dnc 打包 `base-devel` 可选依赖。
  `bash -n`、`.SRCINFO` 重新生成比对、`pacman -T` 和本地文档链接检查通过。
- 在独立干净源码副本上执行 xtask prepare，两份补丁反向检查通过；固定锁文件的
  `cargo fetch --locked --target x86_64-unknown-linux-gnu` 通过，mirror 未修改。
- 原有参数下工作区 33 项测试通过；两个既有 Linux release 后端分别再次通过
  10 项 runtime 和 4 项 Node-API/FFI 测试，CEF API 14900 检查通过。
- 使用既有已验证产物执行两份配方的真实 `makepkg --repackage --force --nodeps`，
  验证 package()、pacman 元数据、互斥/provides、许可证和安装文件清单。
  没有安装或替换系统软件。
- 曾额外运行 `makepkg --noextract --force` 验证完整构建；本机 makepkg 的
  `RUSTFLAGS=-C opt-level=3 -C target-cpu=native` 与原有参数不同，触发 Deno 重编译。
  system-CEF 编译完成后，在工作区测试重编阶段停止；WebView 完整构建未启动。
  不将这次未完成的流程记为完整 makepkg 或干净 chroot 验收。V8 仍使用预编译库。
  `dist/dnr`、`dist/dnc` 恢复为原有已验证的 system-CEF 产物。
- 本轮未重新执行 GUI、macOS 或 AUR 服务器上传。发行内容为源码和 AUR 配方，
  不上传本机按 native CPU 参数构建的二进制。日志与本地测试包位于 `dist/validation-aur/`。

## Linux 并行脚本测试 ETXTBSY 修复（2026-09-18）

用户在本地 PKGBUILD 的 `check()` 遇到 `missing_runtime_exits_before_zip_bytes`
启动失败：`Text file busy`（ETXTBSY）。写句柄在 pack 返回前已经关闭；两个测试并发
fork/exec 时，子进程可能短暂继承另一个测试的可写脚本句柄，触发 Rust/Linux 已知
竞争（rust-lang/rust#114554）。修复只对两个“创建并直接执行脚本”的测试加互斥锁，
保留直接执行、参数传递、缺失 runtime 退出码等断言，其他测试继续并行。

- 原有 16 项包测试以 16 线程重复执行，第 12 轮在另一脚本测试复现同样 ETXTBSY。
- 修复后连续 300 轮、共 4,800 项包测试通过，仍使用 16 线程。
- 工作区 33 项测试通过，15 项宿主测试按设计忽略；Clippy 和格式检查通过。
- 同步测试修复到用户已构建的 `packaging/aur/dnr/src/dnr`，保留其编译产物以便
  用 `makepkg --noextract` 重试；本轮没有重编 Deno 或修改系统安装。

## Arch 标签构建工作流与二进制配方（2026-09-18）

新增仅在版本标签 push 时运行的 `release-arch.yml`。两个 job 分别在 Arch 官方
`ghcr.io/archlinux/archlinux:base-devel` 镜像中，以普通用户构建 CEF/WebView 包并运行
工作区及 runtime/native 测试；全部成功后更新 GitHub Release。产物带单包 SHA-256、
统一 SHA256SUMS、容器镜像摘要及工具链/系统库记录，面向通用 x86_64。

新增 `dnr-bin`、`dnr-webview-bin` 及 `.SRCINFO`。配方从对应 GitHub Release 下载包和
校验文件，在提取前验证 SHA-256，仅重新封装二进制、许可证与文档，不执行编译。

- actionlint、Bash 语法、Git 空白检查通过。
- 本地使用两份既有已验证包，实际完成两个 `-bin` 的 makepkg 重新封装；确认二进制
  逐字节一致、包名/依赖/provides/conflicts 正确，`.SRCINFO` 与生成结果一致。
- 错误摘要与错误文件名的校验文件均在提取前被拒绝。
- 首次沙箱内保留源文件所有权失败，改为复制时不保留所有权后两份配方通过。
- 提交时尚未执行远程 Actions；实际构建、测试和发布状态以对应 tag 的 Actions
  运行记录为准。本地配方验证不等同于 CI 或真实 GUI 验收。

### GitHub Actions 实际构建与下载复验

- [运行 35341683353](https://github.com/fansion314/dnr/actions/runs/35341683353)
  的 CEF/WebView 两个 job 构建、测试及包归档均成功。首次发布 job 因重建标签后
  原 Release 变为草稿、脚本提前设置 Latest 而收到 HTTP 422。
- 恢复原 Release 的公开状态后，仅重跑发布 job，14 秒完成，整次运行最终 Success；
  复用已通过测试的两个归档，没有重编译。脚本修复为上传完成并发布后才设置 Latest。
- 从公开 GitHub Release 实际下载两种归档和 SHA-256，通过两份 `-bin` 配方生成
  pacman 包。下载到的两种 dnr 分别再次通过 10 项 runtime 和 4 项原生插件测试。
- 此次发行二进制来自 tag `v0.1.0` 的 `e4d7eb8`；后续发布脚本修复不改变运行时源码。
  本地下载复验证据位于 `dist/validation-github-bin/`。


## v2 原生分组、持久缓存与跨平台元数据（2026-09-20）

环境：本机 macOS ARM64（Darwin arm64），Rust 1.98.1；固定 Deno `abd22074e4`、
Laufey `1fe8787`，WebView 后端。sccache 保持默认启用。

- `cargo test --workspace --offline`：43 项通过，17 项宿主测试按设计忽略。
- `cargo clippy --workspace --all-targets --offline -- -D warnings`、格式与 Git 空白检查通过。
- 在独立临时项目和固定 mirror 的干净源码上运行真实 `xtask prepare`，最终补丁应用通过；
  当前准备树的反向补丁检查也通过。Deno/Laufey mirror 的 Git 状态均干净。
- debug 构建和最终 `xtask build` release 均通过。`dist/dnr` 与 `dist/dnc` 为 ARM64
  Mach-O；dnr 经精简、ad-hoc 签名和 `codesign --verify --verbose` 验证。
- 最终 release 上显式运行 `runtime`（10）、`runtime_native`（4）、`runtime_groups`（2），
  共 16 项通过。覆盖 v1 兼容、真实 Node-API/FFI、相邻依赖动态库、组内资源路径、
  Deno/Node 同步与异步子进程、四进程冷启动竞争、重复启动复用、旁置安装、
  VFS 可执行权限和映射路径的同步/异步磁盘回退；缺失文件的普通写入落到包旁而非缓存。
- 新增包测试覆盖平台变体的逻辑路径选择、整组落盘、符号链接闭包、严格原生用途声明、
  缓存损坏修复、内容更新身份隔离、清理跳过使用中组、其他平台专属组不参与本机安装。
  重建 ZIP 使 CRC 正确但文件内容与索引不符时，普通读取、原生解压和完整导出均拒绝。
- 使用 `dnc` 将 `examples/desktop/smoke.ts` 打成 v2 包，从 `/private/tmp` 用最终 release
  运行。真实 WebView 自动检查页面标题、JS binding、页面按钮调用和关窗后异步收尾，
  输出 `DNR_GUI_OK`，退出码 0；不是仅检查 HTTP 监听。
- 独立临时目录安装 esbuild 0.28.2（不执行安装脚本），扫描并检查生成的分组配置后打包。
  最终 release 的同步和异步 transform 均输出 `ESBUILD_DNP_OK`。直接运行仅准备实际使用的
  `@esbuild/darwin-arm64` 组；旁置安装运行不创建用户缓存。

真实 esbuild 包性能观察（同机最终 release，仅用于本轮对照，不代表跨机器基准）：

| 场景 | 观察 |
| --- | --- |
| 空原生缓存首次运行，1 次 | 555.25 ms |
| 缓存命中，5 次中位数 | 155.74 ms |
| 复制包并预热全部当前平台组 | 112.29 ms |
| 运行旁置安装，1 次 | 374.94 ms |

五次缓存命中前后，所有原生缓存文件的 inode、mtime 和大小完全一致，确认没有重写。
校验仍会读取组文件，运行时启动及 esbuild 自身工作也计入上面的耗时。原始数据与日志在
`dist/validation-v2/`，不纳入源码提交。

边界：本轮未执行 Linux x86_64 的原生构建/GUI、system-CEF、macOS 薄应用安装器或 Arch
makepkg 流程；此前 Linux v1 记录不能替代 v2 验收。Windows/Linux ARM64 只有可扩展的
平台元数据，不是新增运行时移植。任意 shell 字符串及脱离宿主管理的外部进程不在透明
路径改写或租约保护范围。系统安装的 dnr/dnc 未替换；使用 v2 包需要新版运行时。

## AUR 独立 dnc 与三产物发布流程（2026-09-20）

本轮在 macOS ARM64 修改并验证打包入口，不是 Arch 原生构建验收。

- 四份 dnr 配方只安装运行时，新增 dnc/dnc-bin 及 `.SRCINFO`。dnc 源码配方只声明
  Git、Rust 构建依赖和 glibc/gcc-libs 运行依赖，使用 base-devel 提供的 C 构建工具；
  不准备 Deno/Laufey，不安装 GUI 依赖。runtime 配方使用 `xtask build --runtime-only`。
- 同一 Actions 工作流并行构建 dnr、dnr-webview、dnc；每包检查可执行文件隔离，
  三个包和摘要全部齐全且校验成功后才调用 GitHub Release 发布。
- 在含其他未提交功能的工作区执行
  `cargo test --locked --offline -p dnc -p dnr-package -p xtask`：47 项通过，17 项忽略。
  Clippy（workspace/all-targets、拒绝警告）、格式和 Git 空白检查通过。
- 独立 `cargo build --locked --offline --release -p dnc` 通过，并用产物将 hello 示例
  打包、检查 ZIP 目录；`xtask build --help` 显示新参数。sccache 始终启用；编译器探测
  曾报权限错误，提权并设置 `CARGO_CACHE_RUSTC_INFO=0` 后构建及参数检查成功。
- 六份 PKGBUILD 的 Bash 语法和 `.SRCINFO` 全字段一致性检查通过；工作流 YAML 解析、
  三项 matrix、release 对 build 的依赖及 AUR README 相对链接检查通过。
  本机无 makepkg，未使用 `makepkg --printsrcinfo` 重新生成比对。
- 本地夹具实际执行六份配方的 package()/prepare()，验证各包只安装对应程序，
  -bin 重新封装前后程序逐字节一致；错误摘要、错误归档名称均在提取前拒绝。
  夹具使用本机二进制、Zstd tar 归档及 GNU 文件操作参数的 macOS 适配，不生成 pacman
  元数据，不代表 Linux ELF、依赖闭包或安装验收。
- 发布脚本夹具验证：缺少 dnc 或 dnc 摘要损坏时，不调用任何 gh 命令；三包齐全时生成
  三项 SHA256SUMS 并进入上传流程。git/gh 使用替身，没有访问或修改远程 Release。
  临时验证脚本及归档位于 `/private/tmp/dnr-aur-validation/`。

本轮未重建完整 runtime，未运行真实 Arch 容器/makepkg、GUI 或 GitHub Actions；没有
推送或重建已有标签。版本仍为 0.1.0；新发行前需同步版本、六份配方和发行说明。

提交前从暂存区导出独立源码快照，排除其他未提交功能。该快照的
`cargo test --locked --offline --workspace`：33 项通过、15 项忽略；Clippy、格式和 Bash
语法检查通过。运行时配方保留已有 runtime/runtime_native 测试，不依赖未提交的
`runtime_groups` 测试。上述 47 项是混合工作区结果，不能作为本次独立提交的测试数。


## v2 原生机制性能优化复验（2026-09-20）

同一 macOS ARM64 / Rust 1.98.1 环境，sccache 保持启用。本轮不改变 v2 格式或 SHA-256
身份算法，优化归组索引、别名解析、按组准备锁和缓存校验；ARM64 启用 sha2 的带能力
检测的加速后端，保留软件回退。没有设置 native CPU 构建参数。

- 使用优化前源码快照、相同 `native_performance.rs`、release 配置及确定性输入对照。
  1,000 组的 10,000 次反向路径映射：3138.109 → 8.819 ms；相同组数下普通文件的
  100,000 次归组查询：29.456 → 1.658 ms。
- 32 MiB 组持久缓存完整校验：73.920 → 17.165 ms；旁置完整校验：146.968 → 16.488 ms。
  优化后直接解压为 90.123 ms。1,000 小文件组直接解压约 2971.954 ms，持久/旁置命中
  为 20.234 / 22.050 ms。首次命中仍校验完整内容，没有用 mtime 或标记文件替代哈希。
- 真实运行时交替顺序测量，预热后每场景取 5 次中位数。1,000 个已绑定组下 20,000 次
  普通文件 stat：16753.773 → 74.906 ms；最终 release 独立复核为 70.985 ms。
  无组 v1/v2 在最终 release 上分别为 51.742 / 51.210 ms。
- 仍有一次性索引成本：10,000 文件样例的无组 v2 总进程约 103 ms，v1 约 87 ms。
  稳态查找不再扫描全部组，但没有据此声称 v2 完全没有初始化开销或对所有磁盘提供耗时上界。
- 工作区 47 项测试通过，17 项宿主测试按设计忽略；Clippy、格式、Git 空白和文档路径检查通过。
  新增确定性计数断言：普通查询零探测、缺失旁置版本根只探测一次、持久/旁置只完整校验一次、
  就绪命中不增加 I/O/哈希计数。覆盖租约提前固定路径，以及缺失、多余文件、目录链接替换。
  另修复并验证其他平台的显式目录不遮蔽当前平台隐式目录的边界。
- 最终 release 构建、精简和 ad-hoc 签名验证通过；最终二进制运行 10 项 runtime、4 项原生库、
  2 项原生分组测试，全部通过。首次 Cargo 测试入口复用了沙箱中的 sccache 版本探测失败，
  设置 `CARGO_CACHE_RUSTC_INFO=0` 重新探测后通过，未关闭 wrapper。
- 真实 WebView 的 v2 包再次输出 `DNR_GUI_OK`，退出码 0；真实 esbuild 0.28.2 同步/异步
  transform 再次输出 `ESBUILD_DNP_OK`，继续复用先前生成的缓存。
- 在独立干净上游源码副本上复跑 prepare，最终补丁可应用；当前准备树反向补丁检查通过。
  mirror 未修改。本轮没有新增 Linux 原生或 system-CEF 验收证据。

基准方法和细表见 `docs/PERFORMANCE.md`，输入、优化前二进制、原始样本及日志在
`dist/validation-native-performance/`。这些结果限定本机和上述负载，区分包层与完整运行时耗时。

## v0.2.0 Linux 原生复验与发行（2026-09-20）

从 origin 快进到 `eccedd1`，验证 v2 原生组、持久缓存和独立 dnc 配方；同步版本为
0.2.0，并把 `runtime_groups` 加入两个 runtime 发行配方的 `check()`。
本机为 CachyOS x86_64 / Linux 7.2.6-1-cachyos-bore-lto、KDE KWin 6.7.5 Wayland、
NVIDIA RTX 3080 / 615.71.09；Rust 1.98.1、GCC 16.2.1、CMake 4.4.3、
GTK 3.24.52、WebKitGTK 2.52.6、CEF 152.0.6-1（API 14900）。

### 准备与工作区

- 保留旧准备树，从固定 mirror 提交重新运行 `cargo run --locked -p xtask -- prepare`，
  两份补丁干净应用，构建后反向 `git apply --check` 通过，mirror 没有修改。
- 全程保留 sccache，设置 `CARGO_CACHE_RUSTC_INFO=0`；未设置 native CPU 构建参数。
  首次构建因宿主缺少 libclang 失败，在项目内解压匹配发行版的 Clang 22.1.8 构建库，
  用 `LIBCLANG_PATH` 和 `BINDGEN_EXTRA_CLANG_ARGS=-resource-dir=...` 指定库和标准头文件。
  首次仅指定库时缺少标准头文件，日志保留。期间宿主 Clang 也已可用。
- 格式检查、Clippy（workspace/all-targets、拒绝警告）通过；工作区 47 项测试通过，
  17 项宿主测试按设计忽略。另显式运行 `cargo test --locked -p dnc --test cli
  arch_package_metadata_and_payload -- --ignored`，真实 makepkg 桌面包测试通过。
  最初误写为不存在的 `--test desktop`，更正后的命令退出 0，初次日志保留。
- 六份 PKGBUILD 的 Bash 语法及 `makepkg --printsrcinfo` 全字段比对通过。
  独立 dnc 配方在隔离的准备目录执行真实 `makepkg --noextract --force --nodeps`，
  build/check/package 均通过；只包含 `usr/bin/dnc`，运行依赖为 glibc、gcc-libs。
  此本机流程复用准备目录并跳过依赖检查，不等同于干净 Arch 容器验收。

### WebView release

- `xtask build` 成功；产物单独保留在 `dist/webview/`。ELF x86-64，无缺失动态库，
  不依赖随附 libdenort 或 Laufey 动态库。
- `DNR_BIN=... cargo test --locked -p dnr-package --test runtime --test runtime_native
  --test runtime_groups -- --ignored`：10 项 runtime、4 项原生库、2 项原生组全部通过。
  包含 v1 兼容、真实 Node-API/FFI、相邻共享库、Deno/Node 同步和异步子进程、资源路径、
  普通读取不落盘、缓存复用、四进程冷启动竞争和旁置安装。
- 真实 GUI 磁盘与 v2 ZIP 烟雾测试均输出 `DNR_GUI_OK` 并退出 0；ZIP 还从 `/tmp`
  经 shell 启动头直接执行成功。hello 包保留空参数、含空格参数及 `/tmp` 调用者 cwd。
- 托盘保活、Deno/Node/异常退出（7/9/1）通过；独立 appId 的两个并行应用验证存储隔离，
  再次启动读取各自持久值通过。GUI 测试检查各自进程组退出，无遗留原生子进程。
- `scripts/test-linux-close.py --repeat 2`：两轮、每轮四种真实 KWin 关闭模式均通过，
  正常退出不发送 SIGINT/SIGKILL，完整进程组清理成功。
- 真实 esbuild 0.28.2 在隔离目录以 `--ignore-scripts` 准备，检查 scan 配置后打包；
  同步和异步 transform 在冷启动、缓存命中、旁置安装均输出 `ESBUILD_DNP_OK`。
  缓存命中前后 payload 的 inode、mtime 和大小不变；旁置运行没有创建用户缓存。
- Linux 最终实现性能测量确认索引查询不随组数线性增长；1,000 小文件组冷准备
  1450.049 ms、持久命中 5.978 ms；32 MiB 组分别为 211.391 / 116.314 ms。
  这不是 Linux 优化前后对照，也不是通用应用启动加速比；完整数据见 `docs/PERFORMANCE.md`。

本轮日志、性能原始样本、测试脚本及本地包位于 `dist/validation-v0.2.0/`。

### system-CEF release 与发行包

- `xtask build --runtime-only --backend system-cef` 成功；产物在 `dist/system-cef/`，
  `dist/dnr` 最终为此版本。首次重试时此前临时 libclang 目录已移除导致失败，
  在本轮验证目录重新准备构建库后成功；没有绕过 CEF ABI 检查。
- `--check-system-cef` 通过，`libcef.so` 来自 `/usr/lib/cef`，ELF x86-64 无缺失依赖。
  同样的 10 项 runtime、4 项原生库、2 项原生组测试全部通过。
- 与 WebView 相同的磁盘/v2 ZIP GUI、shell 直接启动、托盘、7/9/1 退出、多应用存储隔离、
  esbuild 冷/暖/旁置运行均通过；缓存 payload 未重写，旁置运行未创建用户缓存。
  `test-linux-close.py --repeat 2` 的八次 KWin 原生关闭全部通过，CEF 子进程清理成功。
- 用两份已验证 runtime 和 dnc，分别执行真实 `makepkg --repackage --force --nodeps`。
  三包的 pacman 元数据和版本正确；两个 runtime 包只包含 `usr/bin/dnr`，dnc 包只包含
  `usr/bin/dnc`。runtime 归档中的二进制与对应验证产物逐字节一致；dnc 按配方正常 strip。
  此处是本机 package() 验证；正式发行由 tag 的 Arch 容器工作流重新构建、测试和发布，
  远端状态以 Actions 和 Release 为准，不上传本机二进制冒充容器产物。
- 六份配方与 `.SRCINFO`、workspace/锁文件和 runtime 版本均更新至 0.2.0；
  发行说明明确 v2 最低 runtime 要求和 dnc 独立安装的变化。

| 本机验证产物 | SHA-256 |
| --- | --- |
| WebView dnr | `9b24d191c43f09c97d1a5fa6992fa844c349df8daa46f2beaeb9fb2d34d235d4` |
| system-CEF dnr | `dbae348b929e11e601475afe9c0c867cf36bcb5310f7cde6853525bcbed91f45` |
| dnc（makepkg strip 前） | `53c1f34baf0f6a42e4be9c17707beeba45c9b08fd33ce10544e2be4093df0df0` |

本轮没有修改 runtime/VFS 实现，没有替换系统已安装的 dnr/dnc，也没有重新安装用户应用。
macOS 证据沿用上文 2026-09-20 的记录；未执行 X11、其他 GPU/发行版或其他架构验收。

### GitHub v0.2.0 实际发布与下载复验（2026-09-21）

- 发行标签 `v0.2.0` 指向 `dccefac`。GitHub Actions
  [35520347358](https://github.com/fansion314/dnr/actions/runs/35520347358) 的三个构建任务和
  发布任务均成功；[Release](https://github.com/fansion314/dnr/releases/tag/v0.2.0)
  已公开包含 system-CEF、WebView、独立 dnc 三个包、单包及统一摘要、构建环境记录。
- 实际下载三个公开附件，单包 SHA-256 与 `SHA256SUMS` 全部通过；核对包内程序隔离、
  0.2.0 版本和 CEF API 14900。发行附件来自 Arch 容器，不是上表的本机验证二进制。
- 两份下载 runtime 分别再次通过 10 项 runtime、4 项原生库、2 项原生组测试。
  各执行一轮四种 KWin 原生关闭模式并启动新版 Songjian v2 包，全部正常退出并清理进程组。
- 三份 `-bin` 配方用真实下载附件完成 makepkg 重新封装，程序逐字节一致，错误摘要和
  错误归档名称均在提取前拒绝。证据在 `github-downloads/`、`downloaded-bin/` 及
  `github-*-runtime.log`、`github-*-close.log`。

| 公开发行附件 | SHA-256 |
| --- | --- |
| dnc-0.2.0-1-x86_64.pkg.tar.zst | `9f6e9e2d84d9bcb95748c3187330a15689fe2f4e8251ee69f0a4636835c1f295` |
| dnr-0.2.0-1-x86_64.pkg.tar.zst | `6df0c8b25d7c0adb04904074560b1c98720a77469fe18c3ff8bdb40af54a589a` |
| dnr-webview-0.2.0-1-x86_64.pkg.tar.zst | `8db66ede2a96f1ddc992d27058e6603ecf0e55caefc1c79be2ac4d717557315c` |

### pi 与 Songjian 的 v2 接入

- pi `0d05e8d4c` / `5eba3bc64` 将默认包改为 Linux x64 glibc + macOS ARM64 两个原生变体，
  显式声明 Node-API 分组和示例脚本组；规范 WASM/许可证的数据权限，保留原有示例内容。
  本轮双平台 DNP 约 6.54 MB，Linux 原生运行通过；macOS 载荷仅检查内容与平台元数据，
  不能据此声称本轮完成 Mac 原生验收。
- pi AUR 只用独立 dnc 及常规小型构建依赖；runtime 仍为最终安装的运行依赖。
  构建期用 Node/libarchive 验证 v2 索引与原生文件并生成旁置组，真实 makepkg 产物布局为
  `/usr/lib/pi/pi.dnp`、`/usr/lib/pi/pi.dnp.unpacked/...`、`/usr/bin/pi` 相对符号链接。
  二进制配方保留文件与模式，错误摘要拒绝。没有把预解压目录放进 bin，也没有安装系统包。
- pi 的完整 `npm run check`、结构测试、两份本机 runtime 的冷/暖/旁置离线烟雾均通过。
  真正从 pacman 归档解出的现有 sidecar 再测通过；下载的正式 CEF runtime 也通过相同测试。
  覆盖真实 Node-API、TS 扩展、Photon、faux 模型/bash、会话和 HTML 导出；没有付费 API 请求。
- Songjian `035dd35` 保持本地打包，加入 dnc/dnr v2 最低版本检查；生成 46,681 字节 DNP
  和 `songjian-1.1.0-2-x86_64.pkg.tar.zst`。前端检查、31 项测试和 4 项 Deno 测试通过，
  10 项旧安装布局测试条件跳过。实际新版包在本机与正式下载 runtime 上原生关闭成功。
- pi 提交已推送 origin 的 `codex/deepseek-responses` 与 GitHub `main`；Songjian 已推送
  origin `main`。pi 源码配方固定到 v2 实现提交，下一次预编译包使用独立打包标签
  `pi-dnr-v0.86.0-2`；本轮仅推送其分支，没有移动旧标签或发布新的 pi Release。

## v0.3.0：2026-09-25 macOS ARM64

v3 格式、路径分代缓存、V8/转译缓存、SQLite 辅助索引、硬链接和两种安装模式已完成本机验证。19 项 runtime/原生测试、Pi 两种部署的离线烟雾、PTY 交互回复和真实 WebView/窗口生命周期通过。

最终同内容 Pi help 基准：关闭缓存 252.33 ms，暖缓存 208.40 ms，首次填充 324.09 ms（中位数，每组 30 次，随机交替）。详细范围、探针边界、原始数据和产物摘要见 [v3 验证报告](docs/V3-VALIDATION.md)。

Linux / system-CEF 本轮未验证，用户已确认保留此状态；未安装远端依赖。没有替换系统安装或发布 v0.3.0。

## v3-only 与上游边界重构（2026-09-25）

本轮按用户要求移除 v1/v2 的读取、输出、JSON 索引和单库临时解压。共用索引校验迁入
`index.rs`，原生库统一使用 v3 声明组；旧格式缓存不再参与管理。CLI 旧包报错提示重打包，
`--format-version` 输出选项删除。v3 的二进制元数据编码与已有 v3 包保持兼容。

Deno 补丁从 20 文件、128 hunks、`+509/-98` 改为 15 文件、110 hunks、`+394/-79`。
ZIP 覆盖辅助逻辑迁入 `integration/rt/dnr_vfs.rs`；编译缓存组合适配器留在 `dnr_cache.rs`，
恢复上游 standalone 缓存文件原状。移除 core 的纯性能探针，保留函数参数参与缓存键的修复。
Laufey 补丁不变。未通过复制整个上游文件来隐藏分叉；边界和升级步骤见 [UPSTREAM.md](docs/UPSTREAM.md)。

- 工作区 55 项非原生测试通过，另有 20 项按设计忽略；fmt、Clippy（拒绝警告）通过。
- 独立 wire-format 夹具继续覆盖路径、链接、CRC 和 SHA-256 校验；新增 v1/v2 明确拒绝测试。
- macOS ARM64 release 构建通过，sccache 保持启用。最终产物统一执行 19 项真实 runtime/原生测试全部通过，
  覆盖 Workers、TS/CJS、FFI、Node-API、子进程、并行冷启动、正常/显式/异常退出的租约释放、
  缓存损坏回退、完整安装、旁置优先和原生文件只读。旧测试中的“缓存目录不存在”改为
  “原生载荷不存在”，保留 v3 编译缓存自动创建行为。
- `scripts/check-upstream-patches.py` 对两个固定 Git 提交执行独立正向应用与反向检查通过；
  当前 `.upstream/deno` 反向检查也通过。mirror 未修改。
- 使用真实旧 Pi v2 包，dnr 执行/tree/install 和 dnc inspect 均明确拒绝，失败安装不创建目标目录。
- 最终 dnr/dnc 下 Pi 的 3 项离线结构/缓存/旁置烟雾通过，包括真实 Node-API、Photon WASM、
  TS 扩展、faux 回复、bash、会话与 HTML 导出。Pi 工作区没有新增修改。初次夹具使用
  `pi-v3.dnp`，旁置烟雾假定文件名 `pi.dnp` 而失败；改用已有同 SHA-256 的 `v3/pi.dnp` 后通过。
- v3 WebView 包输出 `DNR_GUI_OK` 并退出 0。窗口脚本两次在 Cmd+H 隐藏步骤失败；串行
  对照系统已安装的 dnr 0.2.0 也在同一步失败。本轮不能确认隐藏、Dock 恢复、最小化与
  标题栏关闭的自动化验收，不能把历史通过结果算作本轮通过；未因此改动窗口实现。

最终产物 SHA-256：dnr `b1154b65751bf70bfae2c7a3e02569bd41591a785cbcd834936cbd51821777fc`；
dnc `9c3cbd484a3de0d977fbbcd96b33c4a1422d87597fe9580e7a418b96bfed09bd`。

更新后的 v3-only 缓存 A/B 脚本通过 1 次预热、每组 2 次采样的烟雾验证，四组输出一致。
数据位于 `dist/validation-v3-refactor-20260925/bench-smoke/results.json`；样本仅验证脚本，
本轮不据此给出新的性能结论。先前 30 次正式测量仍对应重构前实现。

Linux x86_64 / system-CEF 本轮仍未验证。未替换系统安装，未提交、推送、打 tag 或发布版本。

## 2026-09-25 Linux 双后端与 AUR 扩展

环境：CachyOS x86_64，Linux `7.2.7-1-cachyos`，KDE Wayland，NVIDIA 驱动
`615.71.09`；Rust `1.98.1`，GCC `16.2.1`。GTK `3.24.52`、WebKitGTK
`2.52.6`、Xi `1.8.3`、X11 `1.8.13`；系统 CEF 包 `152.0.6-1`，API
`14900`。保留系统 sccache 与已配置的 clang/mold。

### 构建与无显示测试

- 独立快照上的 `python3 scripts/check-upstream-patches.py` 通过（退出 0）。
  旧 `.upstream` 的 Deno 补丁已过期，首次 prepare 失败；保留为
  `.upstream/{deno,laufey}-before-dual-20260925` 后，从固定 mirror 提交重新
  `cargo run --locked -p xtask -- prepare` 成功，未修改 mirror。
- `CARGO_BUILD_JOBS=6 CARGO_PROFILE_RELEASE_DEBUG=0 cargo run --locked -p xtask -- build --backend dual`
  成功（退出 0）。`dist/dnr --version` 报告 `backend dual`；`ldd` 同时列出
  `/usr/lib/cef/libcef.so` 与 `libwebkit2gtk-4.1.so.0`，没有缺失库；
  `dist/dnr --check-system-cef` 通过（退出 0）。构建保留上游 unused-variable
  与 clang `-pthread` unused-argument 警告，无构建错误。
- 三个配置 dual、system-cef、webview 的原生 CMake 静态库均编译并完成测试宿主
  链接；单 WebView 宿主不链接 CEF，单 CEF 宿主不链接 WebKitGTK。完整 Rust
  runtime 本轮构建的是 dual，不能将这两项原生库检查算作两个单后端的完整验收。
- `cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`
  通过（退出 0）；`cargo test --locked --workspace`：58 通过、21 显式忽略。
- `bash scripts/test-backend-selection.sh`：10 个分派场景通过，覆盖默认优先级、
  ABI/资源/初始化失败回退、初始化前提前返回、显式选择、初始化后禁止切换及 CEF helper。
- `DNR_BIN="$PWD/dist/dnr" cargo test --locked -p dnr-package --test runtime --test runtime_native --test runtime_groups --test runtime_cache --test runtime_backend -- --ignored`
  20 项通过（退出 0），包括 Node-API/FFI/缓存、磁盘与 DNP 入口参数保留，
  应用 `--type=renderer` 参数不会被分派为 CEF 子进程。

### 真实 GUI 与故障回退

`python3 scripts/test-linux-backends.py --dnr dist/dnr --logs dist/validation-dual-20260925/backends`
通过（退出 0）：默认 CEF、显式 CEF、显式 WebView 均检查页面实际引擎并输出
`DNR_BACKEND_OK` 与 `DNR_GUI_OK`。临时 `LD_PRELOAD` 测试库分别模拟 CEF
ABI 不匹配、必需资源不可读和 `cef_initialize` 返回失败；三种情况均自动回退
WebView，窗口绑定与异步收尾成功。显式 CEF 遇 ABI 不匹配退出 78；显式 WebView
不受模拟 CEF 故障影响。严格 ABI 检查在故障下退出 78，普通无显示 CLI 仍退出 0。
所有测试核对退出状态与测试进程组清理，没有遗留原生子进程；没有修改系统 CEF。

`dist/dnc examples/desktop --entry smoke.ts --app-id com.example.dual-smoke -o dist/validation-dual-20260925/smoke.dnp`
生成 v3 包。使用同一 GUI 脚本的 `--package` 参数复验，全部 10 个场景通过（退出 0），结果记录在
`dist/validation-dual-20260925/package-backends/`。

`python3 scripts/test-linux-close.py --dnr dist/dnr --backend system-cef --logs dist/validation-dual-20260925/close-cef`
与 `--backend webview --logs dist/validation-dual-20260925/close-webview`
均通过（退出 0）。两后端各覆盖 Deno 显式退出、Node 退出 9、异步服务收尾和
空闲关闭；使用 KWin 原生窗口关闭请求，全部子进程在约 0.12–0.23 秒内清理。

### AUR 与验证范围

默认 `dnr`/`dnr-bin` 改为 dual，新增 `dnr-cef`/`dnr-cef-bin`，保留
`dnr-webview`/`dnr-webview-bin`；六种 runtime 互斥。发布矩阵与产物检查增加
CEF 专用变体，共三个 runtime 加独立 dnc。八份 PKGBUILD 的 `bash -n` 和
`makepkg --printsrcinfo` 与 `.SRCINFO` 对比均通过，CI shell 脚本语法通过。
本轮未执行完整 makepkg 构建/安装或 GitHub/AUR 发布；未验证 macOS。

回退覆盖可返回的初始化错误；两套 ELF 系统依赖仍须安装，未将缺失动态库或
进程级崩溃记为可恢复情形。没有更换系统已安装的 dnr/dnc，未提交或推送。
构建与 runtime 日志保存在 `dist/validation-dual-20260925/`。

产物 SHA-256：
- dnr：`e41de78b03016f3a87d8e59b2d9f51956141d61fedde551bcb5a8757f2b8067a`
- dnc：`a8820ba21772f1cb721d566d668afec0151eb36fe116953a55ce25f51b4c52fc`

发布前补充：dnc 的 Linux 启动器现在将 desktop manifest 的后端显式传给
`dnr --backend`，防止 WebView 应用在双后端 runtime 下默认选到 CEF。
`cargo test --locked -p dnc`（4 通过、1 忽略）、格式检查及 dnc release 重建通过。
该补充只改变 packager；上面 dnr runtime 的产物及验证不变。
