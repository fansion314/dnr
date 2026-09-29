# 验证记录

## v0.5.2 窗口居中与缓存清理候选验证（2026-09-29）

- 从固定 Deno/Laufey 源码重新准备；两份补丁在独立干净快照上应用及反向检查通过，重复 `xtask prepare` 通过。旧 `.upstream` 缓存树保留为 `deno-pre-v052` 与 `laufey-pre-v052`，未作为实现来源。
- macOS ARM64 WebView debug runtime 构建通过，`dist/dnr --version` 与 `dist/dnc --version` 均报告 0.5.2。`python3 scripts/test-macos-window-center.py --dnr dist/dnr` 真实 GUI 两次启动：初次 760×520 位于 `(580,159)`，保存并恢复 880×600 后位于 `(520,119)`；两个中心点均为 `(960,419)`。显式 `(200,180)` 坐标在创建及随后 `setSize(900,620)` 后保持不变。仅验证了本机当前显示器布局。
- macOS `cargo test --locked --workspace`：74 项通过、29 项按设计忽略；严格 Clippy、格式检查及缓存管理 7 项通过。独立临时目录中的真实 DNP 运行生成缓存后，`--path '*.dnp'` dry-run 选中该来源；删除包后 `--trace` JSON dry-run 显示 `would-remove`，实际清理显示 `removed`。发布 helper 18 项通过。
- xjtuse-arch-dev 原生 Linux x86_64 增量同步后运行 `cargo test --locked -p dnr-package --test cache_management`：7 项通过，覆盖单层/跨层路径通配符、DNP 内容换代、缺失来源、损坏来源保留、完整安装描述与租约清理规则。Linux GUI 本轮未重测；macOS 专属居中分支不会在 Linux 执行。
- 八份 AUR `.SRCINFO` 用 Arch 非 root `makepkg --printsrcinfo` 重建。尚未执行 v0.5.2 Release/Actions 构建、发布、安装或 Linux runtime 全量复验；这些结果不计为通过。

## v0.4.3 应用缩放持久化（2026-09-28）

- macOS ARM64 WebView 和 xjtuse Arch Linux x86_64 dual 的 debug runtime 构建通过，
  固定 Deno/Laufey 版本不变。保留 sccache；本机使用 `CARGO_CACHE_RUSTC_INFO=0` 刷新失败的编译器探测缓存。
- 两平台 `DNR_BIN=... cargo test --locked -p dnr-package --test runtime_zoom -- --ignored`
  各 2 项通过，覆盖原有全局配置/API/事件、API 修改后立即退出再启动、脚本身份隔离、
  DNP appId 隔离和包移动、完整安装与 DNP 共享设置、独立 global 乘数、reset 及损坏文件修复。
- `scripts/test-zoom-persistence.py` 在 macOS WebView、Linux WebView、Linux system-CEF
  均通过：真实 Cmd/Ctrl+= 将倍率 1 改为 1.1，下一次窗口启动恢复；API 改为 1.5 和
  重置为 1 后分别重启恢复。全局倍率保持 1.25，800px 窗口的实际 CSS 视口约为
  640→581/582→426→640，四次启动均验证。
- Linux 在独立 KDE X11 verify 会话、Mesa 软件渲染中验证；CEF 的首次 KWallet 设置向导
  通过测试脚本读取实时控件位置并取消。未测试 Wayland 或硬件 GPU。
- 此处记录版本号更新前的同一功能源码 debug 验证，不预写 v0.4.3 Release 构建结果。
  发布后构建、包校验及 Homebrew 下载安装结果由 Actions/Release 保留。
- 更新版本号后 macOS ARM64 debug 重建，`runtime` 11 项、`runtime_zoom` 2 项、
  `runtime_sync` 1 项全部通过。workspace tests、Clippy（`-D warnings`）、格式检查、
  18 项发布 helper tests 通过。macOS 发布 CI 已包含 `runtime_sync` 原生回归。

## v0.4.2 干净构建修复与重新发布准备（2026-09-27）

- v0.4.2 macOS CI 成功编译 dnc，但 runtime 报告 `--locked` 失败；Arch 三个 runtime
  任务同样失败，未进入 Release 发布。失败记录保留；按用户明确要求保持 0.4.2，尚未发布资产的标签改指修复后的提交。
- 在嵌套于项目 Git checkout 的干净源码快照中复现：未初始化 Git 的 `.upstream` 会让
  `git apply --reverse --check --verbose` 输出全部 `Skipped patch` 却退出 0，导致漏补丁。
- `prepare` 现在为复制的上游树建立独立 Git 上下文。6 项 xtask 测试通过，包括新增的
  嵌套仓库内真实应用补丁与重复 prepare 回归。
- 独立的固定 Deno/Laufey 快照重新 prepare 后，两个补丁反向检查通过，dnr 入口与依赖
  确实出现在生成 manifest；`cargo fetch --locked --target aarch64-apple-darwin` 退出 0。
- 修复后 v0.4.2 的完整构建、原生测试、Release 和 tap 验收交给 Actions；此处不预写成功结果。

## v0.4.2 macOS 预构建与 Homebrew 发布准备（2026-09-27）

- 17 项发布脚本测试通过，覆盖实际归档 SHA-256、固定版本 URL、拒绝坏校验和/移动标签、
  同版本资产不可替换、幂等重跑、tap 防降级与普通 Git push 更新路径。
- actionlint 1.7.12 检查 macOS/Arch 两份工作流通过；新增 Bash 脚本语法、生成配方的
  Ruby 语法、`cargo fmt --all -- --check`、版本和锁文件一致性检查通过。
- 使用当时已有的 macOS ARM64 `dist/dnr`（0.4.1 zoom debug）和 `dist/dnc`（0.4.1），
  打成一个临时 gzip tar 包。架构、只链接系统库及两份 ad-hoc 签名验证通过。
- 在临时 tap 实际执行 Homebrew 安装（不链接全局命令）与 `brew test --force`，
  两份程序版本、TypeScript、包内资源和 `dnc` → DNP → `dnr` 执行通过；测试包随后卸载。
  此结果验证分发/安装路径，不冒充 v0.4.2 Actions release 产物验证。
- 此提交尚未执行远端 v0.4.2 构建、上传或从 Release 下载验收；后续状态、正式哈希与
  tap 更新证据保留在 Actions artifacts / job 日志及 GitHub Release，不追加验收提交。

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


### v0.3.0 GitHub 发布与本机升级（2026-09-25）

- 发布提交 `8e1d882`，标签 `v0.3.0` 已推送 GitHub 与 origin。
  [Actions 36115087620](https://github.com/fansion314/dnr/actions/runs/36115087620)
  全部成功，发布 dual、CEF-only、WebView-only 和 dnc 四个 Arch 包及校验文件。
- Pi 提交 `7d1a80aaf`、新标签 `pi-dnr-v0.87.1-2` 已推送两个远端。
  [Actions 36118415944](https://github.com/fansion314/pi/actions/runs/36118415944)
  成功，使用 dnc 0.3.0 生成 DNP v3。原 `0.87.1-1` 标签和资产未改动。
- 两份 GitHub pacman 资产均通过其发布 SHA-256 校验，再由对应 `-bin` 配方
  本地重打包。下载的 dnr 正式二进制通过 10 个真实 GUI/故障回退场景。
  从最终 pi 安装包重新解出的文件通过 3 项结构、Node-API、缓存/旁置离线测试。
  首次误用 makepkg 收尾后的 pkg/ 工作目录测试，原生文件权限被该目录的清理过程
  调整而失败；最终归档中的权限正确，重新解包后全部通过，无需修改发布资产。
- Songjian 提交 `79a9ede` 已推送 origin，新构建 `songjian-1.1.0-3` 使用 v3，
  新包通过版本检查、前端/桌面测试和真实 KWin 原生关闭验证。
- 使用 `paru --sudo /usr/bin/pkexec -U --noconfirm` 一次安装三个已验证的本地包。
  当前为 `dnr-bin 0.3.0-1`、`pi-dnr-bin 0.87.1-2`、`songjian 1.1.0-3`。
  `pacman -Qkk` 分别检查 24、44、11 个文件，全部 0 altered；命令均从 `/usr/bin` 解析。
  `dnr --version` 报告 format 3 / backend dual，CEF API 14900 检查通过，
  `PI_OFFLINE=1 PI_TELEMETRY=0 pi --version` 输出 0.87.1，退出 0。
- 安装后的 runtime 和两个 DNP 均与已验证的正式/本地产物逐字节相同：
  dnr SHA-256 `153cdc5f1f09ef66a30444b0ba843800df731f15ef958f993ebd7ef1022e856c`；
  pi `dfcb70c40e07e7a11f00b3331928ec8f6ee9cb1ed3cbd28f3c1a40a647f08061`；
  Songjian `18be8f7ecc4b442f6268d1659c632d64c044e0f1100020d8d2c2639c3f356904`。

安装包保存在 `dist/release-install/`，正式 runtime GUI 日志在
`dist/validation-release-v0.3.0/`。这里只发布 GitHub 资产并使用本地配方安装，
未向 AUR 服务器提交仓库；未更改用户应用数据。

## 未发布的 Linux GUI 库延迟装载试验（2026-09-25）

在本机 Apple `arch-dev` 容器的 Arch Linux x86_64 用户态验证；宿主内核仍为 ARM64，
经 Apple 的 Intel 二进制翻译执行，**不是原生 x86_64 内核或真实 GUI 验收**。
固定 Deno `abd22074e4`、Laufey `1fe8787` 的独立副本位于容器 `/root/mirror/`；
dnr 源码快照在 `/root/dnr-lazy`，原始 mirror、仓库未提交的性能文档和 pacman 已安装的
`dnr-bin 0.3.0-1` / `pi-dnr-bin 0.87.1-2` 均未替换。

当前源码保留两个 Laufey 后端静态编入同一个 ELF。Linux 构建从真实后端对象、
CEF wrapper 与系统库导出自动生成 x86_64 跳板和分后端导入表；仅在 GUI 初始化、
CEF helper 或显式 ABI 检查时 `dlopen` 对应系统库。生成器拒绝数据、IFUNC、弱引用、
显式版本化及归属含糊的 GUI 导入；C++ `_Z*` 符号继续由普通 C++ 运行库解析。

- `cmake` dual 原生构建通过；`python3 integration/native/tests/test_gui_imports.py`
  的 9 项真实 ELF 测试、`bash scripts/test-backend-selection.sh` 的 15 个分派场景通过。
- `CARGO_BUILD_JOBS=3 cargo run --locked -p xtask -- build --debug --runtime-only --backend dual`
  完整构建通过；最终产物 `dist/dnr` 为 Linux x86-64 PIE、未精简 debug ELF，
  SHA-256 `d2281290b091517ba819919f6060318d9aaaa4ee759fe1fb9b9f1b0dff3cfbf0`。
  `readelf -d` 仅见 `libstdc++`、`libz`、`libgcc_s`、`libm`、`libc` 和 ELF loader，
  没有 CEF、WebKitGTK、GTK、GLib 或 X11 的 `DT_NEEDED`；`ldd` 无缺失依赖。
- 在普通脚本、显式 `--backend system-cef` 和 `--backend webview` 的纯 CLI 路径中，
  `/proc/self/maps` 探针均输出 `DNR_NO_GUI_LIBRARIES`，退出 0。新 runtime 运行
  已安装的 Pi v3 包 `--version` 与 `--help` 均退出 0；`--check-system-cef` 按需加载
  系统 CEF 152.0.6，并通过 API 14900 hash 检查。
- 无显示环境显式启动 WebView 后进入预期 `no usable graphical display` 错误，
  没有缺库/缺导出错误。临时 `LD_PRELOAD` 测试 shim 拒绝 CEF `dlopen` 时，
  `auto` 明确回退 WebView，显式 CEF 退出 78 且不回退；这不等于页面 GUI 验收。
  另一临时 shim 拒绝 WebKitGTK、JavaScriptCore 和 Soup 装载时，显式 CEF ABI
  检查仍退出 0，证明该路径无需另一套后端库。
- 修复 `--exclude-libs,ALL` 在 debug 链接中隐藏 Node-API 导出的问题后，
  `xtask` 对照上游完整导出表和 GUI `DT_NEEDED` 黑名单检查最终 ELF；
  `napi_create_int32` 等符号重新出现在
  `.dynsym`。真实插件测试及可在 root 容器运行的 17 项 runtime/原生/后端测试通过；
  另以非 root 用户运行只读目录插件测试通过。`cargo fmt --all -- --check`、
  `cargo clippy --locked --workspace --all-targets -- -D warnings`、
  `cargo test --locked --workspace` 通过。

**保留的失败与范围：** `runtime_cache::v3_cache_workers_cjs_extensions_and_full_install`
在本次 debug ELF 下稳定报告暖缓存 `code_misses=1`，已发布的旧 release ELF 则通过。
隔离 A/B 显示：Worker 发出首条消息后立即被终止时，debug 冷运行未落盘
`worker.ts` 字节码，下一次运行多 1 次 miss；延迟消息 20 ms 后冷运行即落盘，
暖运行 miss 为 0。符合 Worker 终止与异步缓存写入的时序竞争，但本轮没有改动缓存实现
或将它归因于 GUI loader。尚未构建新 release、未在真实图形会话验证双后端窗口，
未更新指向旧 v0.3.0 发布二进制的 AUR 配方，也未更换容器的 pacman 安装。
验收后清理本次任务独立的约 19 GiB Cargo `target` 中间文件，并运行
`container clean arch-dev` 回收宿主空间；`/root/dnr-lazy/dist/dnr`、固定上游副本和
原生导入表构建目录仍保留，后续完整 Rust 构建需重新生成 target。


## v0.3.1：Linux 双后端按需加载（2026-09-25）

从 origin 快进至 `d786abb` 后，在本机 CachyOS x86_64 / KDE Wayland / NVIDIA
RTX 3080 完成原生验证。Linux `7.2.7-1-cachyos`、Rust `1.98.1`、GCC `16.2.1`、
GTK `3.24.52`、WebKitGTK `2.52.6`、CEF `152.0.6-1`（API 14900），保留 sccache、
clang/mold。日志与三种独立产物位于 `dist/validation-v0.3.1/`。

### 实现与发行约束

- 修正试验提交对所有 Linux 变体启用懒加载的问题。仅 dual 生成导入表、链接跳板并
  `dlopen` GUI 系统库；单后端恢复 pkg-config/CEF 直接链接。
- 懒加载 CMake 规则移至 `integration/native/GuiImports.cmake`，ELF 验证移至
  `xtask/src/linux_artifact.rs`。最终产物必须保留全部上游 Node-API 导出；dual 不得
  包含 GUI `DT_NEEDED`，单后端必须直接依赖所选引擎且不得依赖另一个引擎。
- 保留一次解析后尾跳转的热路径，不增加逐调用符号查询或互斥锁。没有改动 Deno/
  Laufey 补丁、VFS、缓存或桌面生命周期实现。独立快照上的两份补丁 apply/reverse
  检查通过；构建同步现有接入源码，未修改 mirror。
- `dnr`/`dnr-bin` 的强运行依赖缩减为 glibc、gcc-libs、zlib；CEF、GTK3、WebKitGTK
  改为可选。CEF 系统包不会自动拉入 GTK，因此文档与 optdepends 明确 CEF 需要
  `cef` + `gtk3`。dual 源码配方仍保留两套 GUI 栈的构建依赖；两个单后端配方保持
  原有强运行依赖。八份配方与 `.SRCINFO` 同步到 0.3.1。

### 功能与真实桌面

- fmt、Clippy（workspace/all-targets，拒绝警告）、58 项工作区测试通过；
  21 项显式原生/平台测试按设计不在普通工作区测试执行。
- `test_gui_imports.py` 的 9 项真实 ELF 测试与 `test-backend-selection.sh` 的
  15 个分派场景通过。dual 的 release ELF 仅需 libstdc++、libz、libgcc_s、libm、
  libc 和动态加载器，没有任何 GUI 库的强依赖。
- `test-linux-lazy.py --gui` 的 11 个场景通过：使用 `LD_AUDIT` 在测试进程中拒绝
  动态加载器查找 GUI 库，不卸载或修改系统库。三种 CLI 后端参数在全部 GUI 库
  不可加载且无显示环境时仍完成 HTTP 服务/fetch；两次 maps 探针均没有 GUI 映射。
  首次窗口前同样无 GUI 映射；缺少 WebView 时 CEF 可用，缺少 CEF 时 WebView 可用，
  auto 正确回退。显式缺库、全部缺库和严格 ABI 检查均退出 78。
- `test-linux-backends.py` 在磁盘和 v3 DNP 各通过 10 个场景，覆盖实际页面引擎、
  绑定、ABI/资源/初始化失败回退。故障注入补充拦截显式 dlsym，适配懒加载；错误
  场景严格要求退出 78，所有场景检查原生子进程退出。
- dual 两个后端各通过四种 KWin 原生关闭场景（Deno/Node 退出、异步收尾、空闲关闭），
  全进程组约 0.07–0.18 秒退出。没有把脚本主动 close 当作标题栏关闭验收。
- 新 dual 直接运行本机已安装的 Pi v3 `--version`/`--help` 通过（版本 0.87.1）。

本轮不新增 macOS、X11、其他 GPU 或发行版验收。系统库缺失通过进程级 ELF 加载器
拒绝模拟；没有声称已卸载本机 GUI 包。后端崩溃仍不是可恢复的初始化错误。

### 三种 release 产物

`CARGO_BUILD_JOBS=6 CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_CACHE_RUSTC_INFO=0 cargo run
--locked -p xtask -- build --backend <dual|system-cef|webview>` 均成功（单后端附加
`--runtime-only`），分别约 3 分钟。保持上游弃用、unused-variable 以及 WebView 链接时
clang `-pthread` unused-argument 警告，没有新增构建错误。

三种产物各自执行 `runtime`、`runtime_native`、`runtime_groups`、`runtime_cache`、
`runtime_backend` 的全部 20 项显式测试，合计 60 项全部通过。上文容器 debug ELF
出现的 Worker 暖缓存时序失败在本机这三份 release 测试中没有复现。

CEF-only ELF 直接需要 libcef 与 GTK，不需要 WebKitGTK；WebView-only 直接需要
WebKitGTK/JSC/GTK，不需要 CEF。两个单后端也各自通过四种 KWin 原生关闭场景。
所有 CEF 产物的严格 API 14900 检查通过。`dist/dnr` 最终恢复为已验证的 dual 产物。

### 性能

`bench-gui-loading.py --samples 30` 完成每组两次预热、30 次随机交替样本。
CLI 中位数为 dual 39.89 ms、CEF-only 58.99 ms、WebView-only 63.56 ms。
包含进程/ELF 启动成本的首个可用窗口：CEF dual/direct 为 261.75/263.11 ms，
WebView 为 346.25/360.80 ms；200 次页面调用分别为 19.21/21.48 ms 和
9.54/9.60 ms。本机这些场景没有观察到明显性能下降。方法、边界和原始样本位置见
[性能记录](docs/PERFORMANCE.md)。

### 本机包与产物摘要

八份 PKGBUILD 的 Bash 语法和 `makepkg --printsrcinfo` 一致性通过。用已验证 dual
执行真实 `makepkg --noextract --repackage --force --nodeps`，生成
`dnr-0.3.1-1-x86_64.pkg.tar.zst`，其 `.PKGINFO` 仅强依赖 glibc/gcc-libs/zlib，
CEF/GTK3/WebKitGTK 均在 optdepend。此处复用本机源码与二进制，只验证 package()
及最终元数据；正式发布由标签触发的 Arch 容器工作流完整重建。

| 本机产物 | SHA-256 |
| --- | --- |
| dnr-dual | `e59a6ca90cebaee9cc6625d3b11930b16ba6d3ed8f77d2e57e22aa9b1640b8c4` |
| dnr-cef | `866c99a1cb72d37c076fc914fef2d33f3594c505cf92b886e24802c1b9994568` |
| dnr-webview | `bf8934abbc09d96d09d6bb5a1ac8ce7432ddcea17e581b184e90cabdb6f6fef2` |


### v0.3.1 正式发布与本机安装

- 发布提交 `6a530c8` 与标签 `v0.3.1` 已推送 origin 和 GitHub。
  [Actions 36148372522](https://github.com/fansion314/dnr/actions/runs/36148372522)
  四个 Arch 构建及发布任务全部成功；
  [Release v0.3.1](https://github.com/fansion314/dnr/releases/tag/v0.3.1)
  包含三个 runtime、独立 dnc、摘要与构建环境记录。
- 下载正式 dual 与 dnc 资产，单包 SHA-256 和统一 `SHA256SUMS` 全部通过；对应
  `-bin` 配方真实执行 prepare/package，最终安装归档中的程序与正式资产逐字节相同。
  `.PKGINFO` 核对 dnr-bin 仅强依赖 glibc/gcc-libs/zlib，GUI 栈均为可选依赖。
- 正式 dual 二进制再次通过全部 20 项 runtime/原生测试、11 个懒加载/缺库场景、
  10 个真实 GUI/故障回退场景，以及两个后端各四种 KWin 原生关闭场景。
- 使用 `paru --sudo /usr/bin/pkexec -U --noconfirm` 安装 `dnr-bin 0.3.1-1` 与
  `dnc-bin 0.3.1-1`。两者从 `/usr/bin` 解析，版本正确；pacman 分别检查 24/20
  个文件，全部 0 altered。系统安装程序与复验产物 SHA-256 相同。
- 安装后严格 CEF API 14900 检查通过，11 个懒加载/缺库/真实窗口场景再次通过；
  已安装 Pi 版本命令输出 0.87.1，新 dnc 成功 inspect 现有 Pi v3 包。

| 正式发行资产 | SHA-256 |
| --- | --- |
| dnr-0.3.1-1-x86_64.pkg.tar.zst | `3edde1c88ea5271664fb0f13498f7133b22061332d33ee2a4c94750765df50be` |
| dnc-0.3.1-1-x86_64.pkg.tar.zst | `71a348782689c58b31c5a556f566ec2b55ece3b9c6ee07a84eb4170282962689` |

已安装程序 SHA-256：dnr `2933a938257c3bbe0ec7c420987bce61120fffaf330e5e4e1617ad2dee14bc7a`，
dnc `2f1ab863afe4ded00be91315d0904cf0a5f4c2664ef1f53f15e17f92bfe22a52`。
资产、安装包、Actions 成功页面与正式产物复验日志保存在 `dist/release-v0.3.1/`。

## 2026-09-26：v4 可选窗口/Dock 图标（未发布）

环境：CachyOS x86_64、KDE Wayland / KWin `6.7.5-1.1`、NVIDIA
`615.71.09`，Rust `1.98.1`、GCC `16.2.1`、GTK `3.24.52`、WebKitGTK
`2.52.6`、CEF `152.0.6-1` / API 14900。保留 sccache、clang/mold。

- 包格式升级为 v4（`DNRMETA4` / `DNRINST4`），图标为可选元数据；普通 CLI
  不需要配置图标。PNG 在打包时解码并缩小为最多 128×128 的 RGBA8，窗口端直接
  读取内存。旧包和旧完整安装须重打包，不新增兼容层。
- CEF 分别设置窗口小图标和应用图标；GTK 在打开显示连接前设置应用身份，并设置
  默认图标。CEF 初始化失败后的 WebView 回退也保留该身份。macOS 增加从元数据
  设置 Dock 图标的代码，原生 launcher 保留 bundle 入口/定位/错误提示，移除图标
  环境变量传递。**macOS 本轮没有原生编译或 Dock 实机验收。**
- `cargo fmt --all -- --check`、workspace/all-targets Clippy（拒绝警告）通过。
  `cargo test --locked --workspace`：61 项通过、22 项显式测试忽略；覆盖 PNG
  解码/透明缩放、无图标包、元数据边界/损坏、图标参与内容身份和完整安装往返。
  固定上游独立 Git 快照的两份补丁 apply/reverse 检查通过，没有修改 mirror。
- 从已安装 Songjian v3 导出准备好的内容，用新 dnc 和原
  `../Songjian/desktop/dnc.json` 执行真实 Arch 打包，退出 0；未改 Songjian 源码。
  最终 pacman 归档中的 DNP 为 v4，appId 为 `world.fansionia.songjian`，名称为
  “松间”，内嵌图标 128×128 / 65,536 字节，DNP 共 112,270 字节。
- 在隔离的数据/缓存目录中，从最终归档取出的 DNP 分别用 CEF 和 WebView 启动，
  刻意移除 `LAUFEY_APP_ID/NAME/ICON`。KWin 两个窗口的 resourceClass 与
  desktopFileName 均为 `world.fansionia.songjian`。用户授权的一次桌面截图中，
  左侧 CEF、右侧 WebView 的标题栏左上角均显示松间树形图标。两个实例随后通过
  KWin 原生关闭正常退出，没有遗留进程组。GTK3/Wayland 此项复验使用已安装的
  Songjian `.desktop` / 图标；不声称独立 DNP 能绕过 compositor 的桌面文件匹配。
- `CARGO_BUILD_JOBS=6 CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_CACHE_RUSTC_INFO=0 cargo run
  --locked -p xtask -- build --backend dual` 首次构建和最终增量构建均退出 0
  （约 8m12s / 3m08s），保留上游 unused-variable 警告。最终 ELF 通过 xtask 的
  Node-API 导出和 GUI 按需链接检查。新 Laufey 干净副本执行 `xtask prepare`
  成功，所有补丁文件与已构建源码逐字节一致。
- 最终二进制执行 `runtime`、`runtime_native`、`runtime_groups`、`runtime_cache`、
  `runtime_backend` 的 21 项显式测试，全部通过（退出 0）。新增测试在 DNP 和
  完整安装两条路径核对元数据名称/appId，同时确认有图标的 CLI 不加载 GUI 库。
- `test-linux-backends.py` 使用有内嵌图标的 v4 smoke 包，10 个场景全部通过
  （退出 0），含实际页面引擎、绑定和 ABI/资源/初始化失败回退。额外 KWin
  windowAdded 监听捕获了 5 个 WebView 场景，三个回退的窗口身份均正确。
  初次辅助检查预期 7 条而只得到 5 条，断言失败；没有采集到两个短暂 CEF
  smoke 窗口，不能用该监听证明它们的身份。CEF 身份使用上面的真实 Songjian
  长驻窗口单独核对，不把辅助采集缺口记为通过。
- `test-linux-close.py` 在两个后端分别通过 4 项 KWin 原生关闭回归，退出状态
  0/9 和异步收尾均正确，全部进程组约 0.07–0.18 秒内退出。测试会话存在
  AT-SPI bus connection refused 警告；没有修改辅助功能设置，视觉验收使用了
  经用户明确授权的一次桌面截图。

最终 SHA-256：dnr `1761c86bba80839654ce1c00a5e1bc118797c0e3c651cb69dafae1b3436e7474`；
dnc `98bf15280c568801f1270302f3ac273d55f512d3543a3310986aa6ab3b836077`；
验证用 Songjian DNP `9e27b6989b11da99408dcbeadfd70af01c7f393f933e852558d5042e583dbb8c`。

本轮产物和日志位于 `dist/validation-icons-20260926/`，真实截图为
`backends-desktop.png`（只保留本地，未发布）。系统已安装 dnr/dnc、Pi、Songjian
及用户数据均未替换；没有提交、推送或发布。普通源码版本号仍为 0.3.1，运行时
明确报告 `format 4`；这不是兼容已发布 v0.3.1 包的新发行版。


### v0.4.0 正式发布与本机升级（2026-09-26）

- 发布提交 `363e13a`、标签 `v0.4.0` 已推送 GitHub 与 origin。
  [Actions 36212803104](https://github.com/fansion314/dnr/actions/runs/36212803104)
  三个 runtime、独立 dnc 构建和发布任务全部成功；
  [v0.4.0 Release](https://github.com/fansion314/dnr/releases/tag/v0.4.0)
  包含四个 Arch 包、摘要和构建环境记录。源码版本已升为 0.4.0，包格式为 v4。
- Pi 提交 `2563e49c4` 与新标签 `pi-dnr-v0.87.1-3` 已推送两个远程。
  [Actions 36214598692](https://github.com/fansion314/pi/actions/runs/36214598692)
  成功，使用正式独立 dnc 0.4.0 生成 v4 DNP 与 Arch 包；CLI 不含图标元数据。
  旧标签与发布资产未移动或覆盖。
- Songjian 提交 `d96ae18` 已推送 origin；新修订 `1.1.0-4` 要求 dnr>=0.4.0，
  独立 DNP 和桌面包均内嵌窗口图标。最终安装包由正式 dnc 0.4.0 生成。
  前端/脚本 31 项测试、桌面 4 项测试、类型与格式检查通过，旧布局 10 项跳过。
- 正式 dnr/dnc 和 Pi 发行资产逐一通过 SHA-256，dnr/dnc 同时通过统一摘要校验。
  本机 `-bin` 配方真实运行 prepare/package；最终归档中的程序与正式资产相同。
  正式 dual 通过全部 21 项 runtime/原生测试及 10 个真实 GUI/故障回退场景。
  最终 Pi 安装归档重新解包后通过 3 项结构/Node-API/冷暖缓存/旁置离线测试；
  正式 runtime 与 Songjian 新包的两个后端均通过真实启动和 KWin 原生关闭。
- `paru --sudo /usr/bin/pkexec -U --noconfirm` 一次升级四个本地包，退出 0：
  `dnr-bin 0.4.0-1`、`dnc-bin 0.4.0-1`、`pi-dnr-bin 0.87.1-3`、`songjian 1.1.0-4`。
  `pacman -Qkk` 分别检查 24/20/44/11 个文件，全部 0 altered。四个命令均从
  `/usr/bin` 解析，已安装的 runtime、packager、两个 DNP 与验证产物逐字节相同。
- 安装后 CEF API 14900 检查通过，Pi `--version` 输出 0.87.1，三项完整离线烟雾
  再次通过。已安装 Songjian 在两个后端启动和原生关闭均退出 0，无遗留进程组；
  这些应用测试使用隔离数据目录，不改用户设置和会话。本轮仍未新增 macOS 验收。

已安装产物 SHA-256：
- dnr: `aefdfc8304a345fc055cd9946084c9d9eb57f78afcd46624295c6243b7789728`
- dnc: `e4152ba6290f4c117ede805bf73751c9d2b6820382c9c5c4ac54e737e63cbf0c`
- pi: `4e77e4d6694b19418579c1ca3d5d6c1b50a8303951ccb33aacefcf8506cfb51a`
- songjian: `9e27b6989b11da99408dcbeadfd70af01c7f393f933e852558d5042e583dbb8c`

正式资产、安装归档、Actions 成功记录与复验日志保存在 `dist/release-v0.4.0/`。
本轮发布 GitHub 并更新已配置的 Git 远程，没有向 AUR 服务器提交配方。


## macOS 托盘与薄应用启动器修复（2026-09-27）

环境：macOS 27.0（26A428）ARM64，Rust 1.98.1，sccache 保持启用。
Songjian 修复基于 `f2935fc`；继续使用原 dnr 0.4.0 / DNP v4
运行时，SHA-256 为 `78e43017826da26919dfcaba652a87a87e345a74e7f9de782f44351357ff1ee3`。

### 根因与方案

- 已安装 Songjian 1.1.1 的旧启动器 `execv` 共享 dnr 后，MenuBarAgent 持续记录
  `process has mismatched pid version`，AppKit 记录 `scene activation failed`。
  原托盘图标确实未显示；直接启动相同 dnr 的探针得到正常菜单栏位置。
- 去掉启动器 AppKit 初始化、改用 `POSIX_SPAWN_SETEXEC` 均未解决；使用
  `posix_spawn` 创建子进程且保留启动器后，托盘位置恢复，原生 bundle 身份保留。
- 最终启动器只负责创建、监督并回收运行时子进程，不初始化第二个 NSApplication。
  kqueue `EVFILT_PROC/NOTE_EXIT` 监视退出，信号事件转发 HUP/INT/QUIT/TERM，保留参数、
  cwd、环境与标准输入输出。初版 sigwait(SIGCHLD) 在 Foundation 辅助线程存在时漏掉
  通知，被真实回归捕获并替换；最终未采用轮询或后台服务。
- Info.plist 新增 `DNRLaunchMode=supervised`；Songjian 构建/安装流程检查该标记。
  不修改运行时、包格式或 Linux 启动方式。
- Songjian 新增透明松树 SVG 与生成脚本，macOS 使用 36×36 PNG（18 点、2×），
  其余平台保留彩色图标。两个桌面构建入口均重新生成模板 PNG。

### 验证与安装

- `cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`
  通过；`cargo test --locked --workspace` 的 58 项非忽略测试通过。
- `CARGO_CACHE_RUSTC_INFO=0 DNR_BIN=$PWD/dist/dnr DNC_TEST_ICON=../Songjian/desktop/icon.icns
  cargo test --locked -p dnc --test cli macos_bundle_real_runtime -- --ignored` 通过。
  覆盖真实 .app 编译/签名、特殊参数/空参数、cwd、应用身份、退出码 37、向启动器发送
  TERM 后运行时处理并返回 23、外部 KILL 子进程后返回 137，以及运行时回收。
- Songjian `vp check`、`vp test`（31 通过、10 跳过）、`vp run check`、
  `vp run desktop:check`（Deno check/lint、4 项桌面测试）通过。
- 使用新 release dnc 构建、签名并安装 `/Applications/松间.app`；原包备份到
  `~/Library/Application Support/dnr/backups/2026-09-27T02-56-48.546Z/松间.app`。
  签名严格验证通过，安装与构建 DNP SHA-256 相同：
  `6e63a3a41c050ba28be2ee0a737f065069fe0dfcc72eb74d403a4f6e89998e41`。
- 新 dnc 已原子安装到 `~/.local/bin/dnc`，签名、版本和构建产物哈希核对通过：
  `533037822f6de70e1bef5dc3977235555ace021d6c07006d9a6701bbed91a459`。
  旧 dnc 备份于 `~/Library/Application Support/dnr/backups/2026-09-27T03-02-24.214144Z/dnc`。
  `~/.local/bin/dnr` 与本轮复用的运行时哈希一致，无需替换。
- 用户确认菜单栏出现正常的小松树；MenuBarAgent 记录新运行时成功注册并连接状态栏
  scene，未出现该进程的 PID 版本不匹配。NSRunningApplication 报告 bundle 为
  `/Applications/松间.app`、ID 为 `world.fansionia.songjian`，GUI 归属于运行时子进程。
- 原生关闭按钮隐藏窗口，菜单栏仍有松间条目；通过原生应用重开恢复同一页面/端口。
  原生应用菜单 Quit 后监督进程与运行时都已退出，随后重新打开应用留供使用。
- 启动器文件 53,056 字节；一次空闲采样为 CPU 0.0%、RSS 6,432 KiB，非长期性能基准。
  工作区数据在安装、GUI 检查及退出后 SHA-256 均为
  `7c45a880b12c48f3a9781c280908a2049e4cee664da944caa52e2b448deb3fa2`。
- 托盘右键自动点击受 UI 工具 `cannotClickOffscreenElement` 限制；本轮未把应用菜单 Quit
  当作托盘菜单退出的验收。未切换系统明暗主题；Linux 原生 GUI 未复验（本轮未改变其图标
  或启动器）。原生运行时库实现没有改动，因此未重复全部 Node-API/FFI 原生测试。


## 上游关闭事件、托盘回调与 Node O_EXCL 修复（2026-09-27）

以下先记录第一阶段的结果与阻塞；后续已获授权的完整 Linux WebView/Plasma 复验见本节末尾。补丁原因、入口和移除条件见
[PATCHES.md](docs/PATCHES.md)，新增 API 见 [WINDOW-LIFECYCLE.md](docs/WINDOW-LIFECYCLE.md)。
验证阶段没有推送、发布或替换已安装 runtime/松间；保留原有性能文档未提交修改。

### macOS ARM64

- 保持 sccache，使用 `CARGO_CACHE_RUSTC_INFO=0 cargo run --locked -p xtask -- build --debug --runtime-only`
  构建真实 runtime。产物 `dist/dnr`（debug）SHA-256：
  `a81e9f576e3b96924d209427afeecabd4e7ba413c7afe753b703bfe184fed9b8`。
  普通源码版本仍为 0.4.0；没有把该调试产物安装到 `~/.local/bin`。
- `cargo fmt --all -- --check`、严格 workspace clippy、workspace tests 通过。
- 显式 `DNR_BIN=$PWD/dist/dnr cargo test --locked -p dnr-package --test runtime
  --test runtime_cache --test runtime_groups --test runtime_native --test runtime_node_flags -- --ignored`
  共 20 项通过。含历史 readFile 所有权回归、Node-API/FFI、Worker/缓存和新增 Node 数值标志的源码/DNP 两种入口。
- `node examples/node-open-flags.mjs` 与新 dnr 都通过同步、回调和 Promise 三种 API。
- `dist/dnr examples/desktop/close-api.ts` 真实 WebView 自动回归通过：取消关闭、处理器重入、
  隐藏后 `isClosed=false`、恢复后同一页面 token 且只加载一次、切换销毁策略、显式 destroy 和幂等调用。
  首版发现 native fast-call 升级覆盖 JS close；改用独立 Symbol 后重复关闭通过。
- macOS 标题栏/快捷键验收**未通过**：现有 `test-macos-window.py` 在 Cmd+H 隐藏步骤超时；
  独立 System Events 标题栏点击也失败，CUA 未识别未打包的 dnr 进程。
  本轮没有把 API 自动测试当成原生关闭按钮验收，也未给该 UI 自动化失败归因到新补丁。

### xjtuse-arch-dev 原生 Linux x86_64

- 环境：Rust 1.98.1，GTK 3.24.52，Ayatana AppIndicator 0.6.0-2，Xfce panel 4.20.8-1；
  Xvfb/Openbox/X11 + 软件渲染，复用已有 session D-Bus。
- 使用规定的增量同步助手，工作区 `/workspace/dnr-92802ab7a212`；排除 macOS `.upstream/`、
  target/dist，固定上游另以只含指定提交的独立 Git 快照同步，并在远端成功执行干净 `xtask prepare`。
  后续测试文件同步来自已登记的 patch，不修改本机 mirror。
- `cargo test --locked --manifest-path .upstream/deno/Cargo.toml -p deno_fs
  --features deno_core/v8 --lib`：5 项通过，包括数值标志映射与真正的 Unix 文件打开。
  单独测试包需要显式开启 `deno_core/v8`；首次省略时报 feature 错误，修正命令后通过。
- `strace -f -e trace=openat` 实测：`O_RDONLY/O_WRONLY/O_RDWR | O_EXCL` 在已存在普通文件上成功，
  syscall 中保留 `O_EXCL` 且没有 `O_CREAT`；`O_RDWR|O_CREAT|O_EXCL` 对已存在文件返回 EEXIST；
  删除文件后单独 EXCL 返回 ENOENT，组合标志可以新建。文件内容未被改变。
  没有访问真实块设备，因此尚未验证块设备 EBUSY、独占保持到 close 或 Etcher 烧录。
- 用 `g++ -std=c++17 -Wall -Wextra -Werror` 编译 `integration/native/tests/tray_activation.cc`，
  include 指向 patched Laufey backend-common/capi，再链接 `pkg-config --cflags --libs gtk+-3.0` 和 `-ldl`。
  `gui-run timeout 15s .cache/validation/tray-activation` 通过，覆盖回调、多图标、替换、移除和无锁重入。
- 带 PNG 参数运行该 fixture，真实窗口关闭后隐藏；真实左键点击**仍打开菜单，未通过恢复验收**。
  原因是 Ayatana 不发布 ItemIsMenu，而 Xfce 将缺省值解释为 menu-only。
  从 watcher 读取实际 bus name/path 后调用 `org.kde.StatusNotifierItem.Activate` 返回成功并恢复窗口，
  证明原来的 No handler for Activate 已被修复。真实右键打开菜单并点击 Quit 正常退出。
  这仅是 patched 托盘源文件的 GTK fixture，不是完整 dnr/松间测试。
- WebKitGTK 与 CEF 都未安装。自动审批拒绝 `pacman -Syu --needed webkit2gtk-4.1`，
  理由是项目禁止自行安装远端系统依赖；该阶段已请求用户授权，当时尚未收到答复。
  未绕过拒绝安装系统包。因此完整 Linux runtime、两个后端原生关闭与松间应用本轮尚未验证。

证据：远端 `.cache/validation/{open-flags.strace,tray-interactive.log,tray-*.png}`；
本地 `dist/validation-upstream-20260927/` 已保存并查看截图。
`xfce-left-opens-menu.png` 是失败证据，`dnr-tray-dbus-restored.png` 是显式 D-Bus 恢复证据，
两者不能互相替代。测试没有修改面板设置、用户松间数据或安装的应用。


### 授权后完整 Linux WebView 与松间复验（同日）

用户明确授权在 `xjtuse-arch-dev` 内安装 WebKitGTK 并同步升级系统包后，执行
`pacman -Syu --needed --noconfirm webkit2gtk-4.1` 成功，安装 WebKitGTK 2.52.6-1 及依赖；
没有改动宿主机系统包。保持 sccache，通过增量同步复用上游快照、Cargo 缓存和构建目录。

- `xtask prepare` 对齐最终补丁成功；`cargo run --locked -p xtask -- build --backend webview --debug`
  完整构建成功，首次 runtime 编译约 5 分钟。产物为原生 Linux x86_64 ELF，N-API 导出检查通过，
  `ldd` 所有依赖均可解析。最终 Linux debug `dist/dnr` SHA-256：
  `a5edb4fbfe5be973c341d5d1987a7d50daa0ee7b3e1f6d25895d5d8b3b2bd389`。
- 显式 runtime/cache/groups/native/node-flags 共 20 项通过。首次运行只读目录测试被其
  “必须非 root”前置条件拒绝，随后以容器 UID 65534 运行该已编译测试并通过；其余测试按原环境执行。
- `close-api.ts` 在 Linux WebView 上通过；最终补充了既有 `attachPanel().destroy()` 在
  hide 策略/取消处理器存在时仍无条件销毁的回归。macOS 最终增量构建与同一 API 回归也通过。
  最终 macOS debug runtime SHA-256：`29a238d45326c09a2c664a2e7c3abeceae9784af797d870ac4a09fc2646fe565`。
  新增 API 来源已在 README、类型注释、WINDOW-LIFECYCLE 和 PATCHES 中明确区分；
  这些仍是未发布的 dnr 扩展，没有升级源码版本号。
- `gui-run python3 scripts/test-linux-close.py --dnr dist/dnr --backend webview --window-system x11
  --repeat 2 --logs .cache/validation/native-close` 通过 8 个 WM 原生关闭场景，覆盖 Deno.exit(0)、
  Node process.exit(9)、HTTP 服务异步关闭和无服务器异步收尾。关闭到进程退出约 0.02–0.18 秒。
  X11 选项使用 WM_DELETE_WINDOW，不调用应用 JS close。首次运行发现容器 PID 1 不回收
  已退出 WebKit 子进程；测试 runner 改为子进程 subreaper 后，按测试进程组回收并验证无活跃残留。
  没有修改系统 init、WebKit sandbox 或 runtime 的退出逻辑来绕过检查。
- 松间源码使用原 checkout 内容，在独立远端副本完成 `vp install --frozen-lockfile`、`vp build`、
  `vp check`、`vp test`（31 通过、10 个旧安装布局测试跳过）、`vp run check`。未修改业务源码。

**Xfce 4.20.8 / Openbox / X11，源码入口：**

- 实际页面截图正常；真实点击“开始专注”，再点击标题栏关闭后隐藏。
- 只读观测端点确认 `id=1, loads=1, closes=1, closed=false, visible=false`。
- 实际托盘右键菜单“显示松间”恢复；恢复后仍是同一窗口、同一页面 token 和相同 URL，加载次数仍为 1。
  计时从 24:56 继续到 23:28，没有重建页面。再次标题栏关闭后，经真实 D-Bus Activate 恢复也保留同一页面。
- 随后另一项环境配置工作切换了桌面，正在运行的源码测试结束。故该源码轮次的菜单退出未计为通过；
  切换完成经用户明确确认后，才继续以下 Plasma 测试，没有与桌面切换并发操作鼠标。

**Plasma / KWin X11 6.7.5 / 软件渲染，DNP 入口：**

- 使用相同 Ayatana 0.6.0-2，无系统库补丁、无面板行为设置改动。
- 以未修改的 `desktop/*.ts` 和远端前端构建输出组成 DNP，包含正式 appId 与窗口图标。
  这是测试打包，不是发行脚本的 minified bundle，也不是对已安装松间的替换。
- 观测版 DNP 仅增加 `app-lifecycle-probe.ts` 入口包装器，记录创建窗口和 load/close 次数，
  对测试页面写入唯一 nonce，并通过随机 loopback 端口提供状态读取；应用模块保持原样。
  使用独立绝对 `XDG_DATA_HOME`，隔离用户应用数据。
- 真实标题栏关闭后窗口隐藏；**真实鼠标左键点击托盘成功恢复**，不弹菜单。
  记录 `id=1, loads=1, closes=1, closed=false`，恢复后 visible=true；页面 token、URL 不变，
  专注计时从 24:57 继续到 24:08。这里使用鼠标输入，不是以手工 D-Bus 调用代替左键。
- 随后用不含探针入口的普通 `Songjian.dnp` 复验：正常页面、标题栏关闭隐藏并保留 PID、
  真实托盘左键恢复、右键“退出松间”，最后退出码 0，runtime PID 消失。
  普通包 SHA-256：`ea93434556c262b17a08a172438c5cff57e17554c2d31c107a63234fdb5e55f9`。
- 最终证据在远端 `.cache/validation/` 的 `songjian-source-*`、`songjian-plasma-*`、
  `songjian-normal-*`、`native-close/`、构建日志和测试日志；截图均取回并实际查看。

结论边界：Linux WebView + Plasma X11 的三个相关路径已有实际运行证据；
Xfce 的 ItemIsMenu 缺省策略限制仍存在。未构建或验证 system-CEF/dual、Wayland、真实 GPU、
实际音频输出或块设备烧录。没有发布/安装新 runtime 或松间，没有修改用户真实数据。

用户在验收后确认当前目标不提供 Xfce 支持；其 ItemIsMenu 兼容性记录作为范围边界保留，不阻塞本轮交付。


### macOS 松间原生关窗页面保留复验（同日，提交前追加）

按用户要求，用本轮 macOS debug runtime（SHA-256 `29a238d45326c09a2c664a2e7c3abeceae9784af797d870ac4a09fc2646fe565`）
和未修改的松间业务源码，重新 `vp build` 并由 dnc 构建独立薄 `.app`。
测试 bundle ID 为 `world.fansionia.songjian.validation.close`，DNRRuntimePath 指向工作区产物；
探针在导入松间入口前把 HOME 指向测试目录，未改 `/Applications/松间.app` 或真实用户数据。

- 通过 CUA 操作真实 AppKit 窗口，点击“开始专注”，在待办输入框留下未提交文本
  `未提交草稿 macOS 保留验证`；没有点击添加。
- 点击原生红色关闭按钮。第二次关闭后，在 UI 工具重新激活应用前读取探针：
  `id=1, loads=1, closes=2, closed=false, visible=false`。
- 页面 token 始终为 `0.1kseyot4606`，服务 URL 始终为同一端口；未保存草稿仍存在，
  计时从 24:37 继续到隐藏时的 23:34，再到重新显示后的 22:57。
- 通过 CUA 重新激活应用后，窗口 visible=true，仍只有一个窗口、一次页面加载；
  可访问性树和截图均确认未提交草稿保留。重新激活不是重建页面。
- CUA 的 `getAXState()` 会重新激活该测试应用，因此第一次关闭后紧接 AX 检查时看到 visible=true；
  为避免把工具的重新显示当作关闭失败，第二轮先独立采样隐藏状态，再进行 AX 检查。
- 本轮补齐了之前未打包进程无法被 UI 自动化定位时缺失的**原生红色关闭按钮**证据。
  结论是当前补丁运行时下松间只隐藏窗口、不销毁页面；不是对仍安装着的旧运行时重新验收。

证据 JSON 位于 `dist/validation-upstream-20260927/macos-songjian/`：
`before.json`、`after-close.json`、`hidden.json`、`restored.json`；截图已在 CUA 中实际查看。


## v0.4.1 发布准备与本机安装（2026-09-27）

发布提交 `a0c6030351ed05b0161dd355d31dc84fc2ee82b6`，标签 `v0.4.1`；
已原子推送到 Gitea origin 与 GitHub，两端 main/tag 解引用均核对一致。
松间的 `47878ee` 已同步到其唯一配置的 Gitea origin。
[Actions 36300571743](https://github.com/fansion314/dnr/actions/runs/36300571743)
由标签触发，四种 Arch 包完成各自检查后由 workflow 自动发布；本地没有手工上传发布包。
当前记录时独立 dnc 构建已成功，三个运行时尚在构建，不能据此宣称 GitHub Release 已发布。

本机 macOS ARM64：

- workspace tests、严格 clippy、fmt、补丁快照检查通过；8 份 PKGBUILD/.SRCINFO 与两份
  Cargo.lock 的版本一致，runtime 版本为 0.4.1，包格式保持 v4。
- 保留 sccache，`CARGO_CACHE_RUSTC_INFO=0 cargo run --locked -p xtask -- build` 的 release
  构建通过；22 项显式 runtime/native/groups/cache/backend/node-flags 测试通过。
  真实 macOS 薄启动器测试及 release `close-api.ts` 回归通过。
- 松间 `vp check`、`vp test`（31 通过、10 跳过）、类型检查、4 项桌面测试通过。
  使用新 release dnr/dnc 重新构建、签名并通过项目安装器安装 `/Applications/松间.app`。
  应用源码仍为 1.1.1；应用 DNP 内容未变，更新的是运行时固定路径及安装产物。
- 已原子替换 `~/.local/bin/dnr`、`~/.local/bin/dnc` 为 0.4.1，签名和构建/安装 SHA-256 相同。
  松间的 `DNRRuntimePath` 指向下述相同 runtime 哈希的共享副本，`DNRLaunchMode=supervised`。
- 实际打开已安装松间，CUA 可访问性树和截图确认页面正常；监听服务 HTTP 200，
  runtime 进程确实从新版哈希目录运行。用户工作区文件在安装前、安装后、启动后哈希均为
  `7c45a880b12c48f3a9781c280908a2049e4cee664da944caa52e2b448deb3fa2`。
- 按用户要求，Pi **没有重新打包或安装**。现有 Pi v4 包在新 PATH/runtime 下输出 0.87.1，
  文件 SHA-256 保持 `ed453378b24d9bcbbbae48c2d141f2e03fb215bb0e1b318f610c37ab58c25459`。

安装产物 SHA-256：

- dnr：`3ede25d3535bfb94643c3bd3cc555b1e6ead1ca85147e67c38fabed06f7233a5`
- dnc：`a3be9a4f4fb58954703fc6c8c53b392d6b05253a6f03aff3d5122729a9ddd225`
- 松间 DNP：`6e63a3a41c050ba28be2ee0a737f065069fe0dfcc72eb74d403a4f6e89998e41`

备份：`~/Library/Application Support/dnr/backups/2026-09-27T06-42-14Z-v0.4.1-tools/`
保存原 dnr/dnc；`2026-09-27T06-42-21.349Z/松间.app` 保存原应用。
本地日志、安装前后摘要及校验记录在 `dist/release-v0.4.1-local/`。
保留无关的 Pi 性能文档修改，没有提交到本轮发布中。


## 用户全局与应用乘算缩放（2026-09-27，v0.4.2 发布前验证）

实现 `dnr zoom [set <factor>|reset]`、启动时用户配置快照、`Deno.desktop` 应用进程倍率
与原生 Cmd/Ctrl 缩放快捷键。全局 × 应用倍率，不重复乘系统 DPI；应用倍率不持久化。
接入保存在两份上游 patch 与 integration 源文件，下游 Laufey C ABI 为 35；包格式仍为 v4。
以下记录形成于功能提交前；当时未推送、发布或替换已安装的 dnr/dnc/应用，测试版本字符串为 0.4.1。

### 构建与自动回归

- 保持 sccache。macOS ARM64 使用 `CARGO_CACHE_RUSTC_INFO=0 cargo run --locked -p xtask -- build --debug --runtime-only`；
  Linux x86_64 在既有 xjtuse-arch-dev 中增量同步、干净上游 `prepare` 后使用 `--backend dual --debug --runtime-only`，均退出 0。
  旧远端生成树保留于 `.cache/zoom-old-upstream/`；没有修改本机 mirror 或复用 macOS 构建工具。
- 用户明确授权后在容器中运行 `pacman -Syu --needed --noconfirm cef cmake clang pkgconf libxi libx11`。
  新增 CEF 152.0.6-1、openh264 2.6.0-2；其余包无需升级。没有修改宿主机系统包。
  `dist/dnr --check-system-cef` 退出 0，API 14900，与 headers 152.0.6+g708dc14+chromium-152.0.7977.83 匹配。
- 本机 workspace tests、严格 clippy、fmt 检查通过；`check-upstream-patches.py` 在固定快照应用/反向检查通过。
  新增配置测试覆盖路径选择、原子写入、缺省、损坏与修复、非法倍率和 I/O 错误。
- macOS 显式 runtime/native/groups/cache/node-flags/backend/zoom 共 23 项通过。
  Linux dual 同组 23 项通过：22 项在容器默认 UID 下执行，只读目录原生插件测试单独以 nobody 运行并通过。
- `runtime_zoom` 实际运行 rebuilt dnr，覆盖 1.25 × 1.2 = 1.5、错误类型、去重事件、重置保留全局、
  配置启动快照、后续进程新值、同名脚本与参数、损坏配置警告/回退。Linux dual 读取 `/proc/self/maps`
  证明无窗口缩放 API 不加载 GTK/WebKitGTK/CEF。既有 backend 参数与元数据无 GUI 测试继续通过。
- 用 `clang++ -std=c++20 -Wall -Wextra -Werror` 编译并执行原生快捷键 matcher fixture，退出 0。
  覆盖按键消费、重复、缺少 printable character、边界匹配和开发者工具豁免。

### 实际页面与原生输入

- macOS 系统 WebView：`examples/desktop/zoom.ts` 实际布局自动验证通过。
  系统基准 DPR=2，全局 1.25 × 应用 1.2 时 DPR=3，800 点内容区域的 CSS 视口为 533 px；
  页面 100 CSS px 色块的 CSS 几何不变。覆盖同站点多窗口、不同站点导航、刷新、隐藏恢复与新增窗口。
- 构建、签名独立 `DNR Zoom Probe.app`，使用隔离 `DNR_CONFIG_DIR` 和测试 bundle ID。
  CUA 在真实 AppKit 窗口的输入框焦点下发送 Cmd+=、Cmd++、Cmd+-、Cmd+0、数字键盘加/减/0；
  实际倍率、三个窗口视口及事件同步正确，启用时页面未收到缩放 keydown，禁用时收到一次且倍率不变。
  缩放后实际点击按钮，可访问性树显示“已点击”；截图实际查看。测试包已正常退出。
- Linux 环境：Rust 1.98.1、GTK 3.24.52、WebKitGTK 2.52.6，KDE/KWin X11、Xvfb 1600×1000、软件渲染。
  使用独立 `GUI_SESSION_SLOT=verify`，串行执行 `scripts/test-linux-zoom.py` 的 webview 与 system-cef。
  两者 HTTP 页面、多窗口、跨站点导航、隐藏恢复、新窗口、实际 CSS 视口测试均通过。
- xdotool 真实 Ctrl+=/+/-/0、数字键盘加/减/0、重复 keydown、上限和禁用开关测试通过，
  且确认输入框拥有焦点。全局配置改为 1.5 后旧进程保持 1.25，新进程读取 1.5 且应用倍率恢复 1。
  截图取回并查看，结构化结果校验视口与控制器倍率一致，测试进程退出 0。
- CEF 测试捕获并修正原始按键无 printable character、数字键盘 VK 映射缺失、
  消费 raw key 后不一定收到 keyup，以及禁用平台快捷键后 Chrome 默认动作绕过控制器的问题。
- 初期 CEF HTTP 加载超时。禁用新增 zoom 调用的对照仍失败，data URL 验证正常；
  截图发现独立 KDE 会话的首次 KWallet 向导。根据实时控件树取消向导后，完整 HTTP/跨站点测试通过。
  最终测试使用 `--cancel-wallet-setup`（只允许 verify 会话），没有创建钱包或关闭系统密码存储保护。
  早期失败保留为诊断历史，不把 data URL 结果当作 HTTP 验收。

### 限制与证据

- 系统 CEF 152 尽管收到 `chrome_zoom_bubble=STATE_DISABLED`，截图仍可见原生缩放提示。
  已通过 CefCommandHandler 将 Chrome 放大/缩小/Reset 命令接入同一控制器；其按钮未在本轮
  可访问性树暴露，因此没有把真实点击这些原生按钮记为通过。键盘重置保留全局倍率已实测。
- 未覆盖 Wayland、真实 Linux GPU、混合 DPI 多显示器迁移、运行中修改系统缩放、渲染进程崩溃恢复，
  也未重建两个 Linux 单后端变体。此次 Linux 两后端结果均来自 dual 构建显式选择。
- 新代码未修改系统显示倍率，也未增加根据屏幕分辨率猜测倍率的策略。
- 本机证据：`dist/validation-zoom/` 的 macos-*.json、macos-bundle.log、临时 .app，以及
  `linux/zoom/{webview,cef-data,cef-http}/` 的 runtime.log、results.json 和截图。
  远端对应 `.cache/validation/zoom/`；失败状态文件仅代表相应诊断轮次。

最终 debug 产物 SHA-256：

- macOS dnr：`9748fc2ba9dc5f2faa1f9e90afc35692e9d57ae805a8c07242a1409d171ca679`
- Linux dual dnr：`758b2d86a5034fa6b5ed6ebe6e9aab1a023c8c0b81cc8eaeb7033aa502900ee7`


## v0.5.0 缓存管理、窗口状态与隐藏释放（2026-09-28，发布前）

DNP 保持 v4；工作区和运行时版本更新为 0.5.0，下游 Laufey C ABI 为 36。
本节只记录发布提交之前已经执行的验证，不预写 GitHub Actions、正式资产或本机安装成功。

### 构建、缓存与接口回归

- 保留 sccache；本机使用 `CARGO_CACHE_RUSTC_INFO=0 cargo run --locked -p xtask -- build --debug`。
  Linux x86_64 在 xjtuse-arch-dev 增量同步源码、从固定 Deno/Laufey 准备干净树后，
  使用 `build --backend dual --debug --runtime-only`。没有将 macOS .upstream/产物用于 Linux。
- 两端工作区测试通过；本机 71 项非 ignored 测试。严格 workspace clippy、fmt 和补丁
  独立快照应用/反向检查通过。18 项发布/Homebrew Python helper 测试通过。
- 本机 26 项显式 runtime/native/groups/cache/backend/node-flags/zoom/state 测试通过。
  Linux 同组检查通过；只读目录原生插件检查单独以 nobody 执行，避免 root 掩盖权限错误。
- 新测试覆盖日期与容量解析、逻辑单位、LRU、活跃路径跳过、未知访问时间、dry-run、
  索引重建保留访问记录，以及清理后索引一致性。使用时间与索引时间分开。
- 新旧包旁布局、原始逻辑资源和符号链接、运行中安装锁/旧组租约拒绝覆盖均通过。
  macOS 测试使用 canonical 路径比较，避免 /var 与 /private/var 的别名造成假失败。
- 设置测试覆盖 5 秒防抖、连续操作合并、无变化不写入、关闭开关取消待写、显式退出补写、
  旧 app-zoom 导入、跨进程字段合并、移动/更新身份延续及不初始化 GUI 的 API 路径。
- 八份 Arch .SRCINFO 在 nobody 拥有的临时目录中用 makepkg 重新生成并逐字节比对通过。

### 真实窗口与资源释放

- macOS ARM64 WebView、Linux dual 的 WebView 与 system-CEF 均通过
  `examples/desktop/window-state.ts`：默认策略、取消 close、keep 保留同页、show 取消计时、
  重复 hide 不延后计时、原生实例实际释放、原对象重建、bindings 恢复及同步保存最终缩放/尺寸。
- Linux 两后端重启恢复 880×600 / 1.25；禁用记忆后使用构造尺寸 760×520 / 默认倍率 1。
  关闭 API 回归覆盖永久销毁、可取消/重入 close、默认隐藏及托盘面板无条件销毁。
- 构建隔离 `DNR Window State Probe.app`，通过 CUA 点击真实 macOS 关闭按钮；应用控制端
  确认 native exists=0、逻辑对象未永久关闭。再次 show 后逻辑 ID 不变、页面 token 改变，
  实际点击重建后的按钮显示 pong:click；实际截图已查看。原生拖动到 830×580 后配置保存一致。
- Linux 使用既有 `GUI_SESSION_SLOT=verify` KDE/KWin X11、1600×1000、软件渲染。
  `scripts/test-linux-window-state.py` 通过窗口管理器发送原生关闭请求，验证释放、恢复、
  新页面 token、稳定逻辑 ID、绑定及进程退出，生成结果和截图；截图已取回查看。
- 固定页面分配 64 MiB 后，进程组 RSS 求和观察值：WebView 从 607016 KiB 降至
  278980 KiB，CEF 从 1171804 KiB 降至 862260 KiB。页面进程/实例释放证据成立；
  RSS 求和会重复计算共享页，不代表独占内存或恒定回收量，CEF 共享进程可以继续存在。
- CEF 初始 about:blank 也会发 load；测试改为按实际页面 token 去重，并等待 load 完成，
  没有用额外空白页事件冒充页面重建。早期失败与修复后结果分别保存。
- 两个 Linux 后端的 HTTP 页面与 binding 烟雾测试通过。CEF 首轮被独立会话的
  KWallet 首次设置提示阻塞并超时；重跑时根据实时可访问性树取消该提示后通过，
  未创建钱包、未关闭系统密码存储保护。失败日志与通过日志分别保留。

### 证据与限制

- 本机证据：`dist/validation-v050/macos/` 的 workspace.log、runtime.log、
  native-close-and-resize.json、隔离 bundle 和配置；Linux 取回记录在同级 linux/、linux-final/。
  远端原始记录在 `.cache/validation/v050/`；此前生成树保存在 `.cache/v050-upstream-before/`。
- xjtuse 曾连接超时；短暂启动本机 arch-dev 检查备用环境后，按用户要求停止。
  本轮 Linux 验收来自恢复连接后的 xjtuse 原生 x86_64，并非 Apple Container 的翻译执行结果。
- 未覆盖 Wayland、真实 Linux GPU、混合 DPI 显示器迁移和运行中 DPI 改变；两个单后端变体
  尚未在本轮本地重建，其 release 构建由标签 Actions 执行。
- macOS 的 `getNativeWindow()` WebGPU 句柄导出返回 unknown Laufey window handle type: 0；
  Homebrew 已安装 0.4.3 对照复现同样限制。原有 surface_taken 保护保留，但此路径不计实机通过。
- 没有替换本机已安装的 dnr/dnc、Pi、松间或其他应用；发布后的资产与 CI 结果不追加验收提交。
