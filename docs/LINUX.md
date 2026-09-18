# Linux x86_64 验收

2026-09-18 已在用户的 CachyOS x86_64 / KDE Wayland / NVIDIA RTX 3080 主机完成 WebView 与 system-CEF 原生构建、原生测试和真实 GUI 自动验收，结果和具体版本见 [VALIDATION.md](../VALIDATION.md)。以下命令用于后续复验；本机按用户要求直接编译，不使用 sccache。

## WebView

需要原生 Linux x86_64、Rust、C/C++ 编译器、CMake，以及以下 pkg-config 项：

```sh
pkg-config --modversion gtk+-3.0 webkit2gtk-4.1
cargo run -p xtask -- prepare --deno /path/to/deno --laufey /path/to/laufey
cargo run -p xtask -- build
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored
dist/dnr examples/desktop/smoke.ts
```

最后一个命令需要图形会话，应看到短暂打开的窗口、`DNR_GUI_OK`，随后退出码为 0。测试成功标记发生在窗口关闭、异步任务完成之后。

在真实 Wayland/NVIDIA 环境中复验。dnr 在初始化后端之前为 Wayland 会话设置 `__NV_DISABLE_EXPLICIT_SYNC=1`。本轮保留真实 Wayland 协议日志，验证当前 KWin / NVIDIA 驱动组合；没有进行禁用该变量的对照实验，也不据此推断其他 compositor / 驱动组合。

## 系统 CEF

首版使用 Arch/CachyOS 系统 CEF 布局。系统需提供 `/usr/lib/cef`、CEF headers、wrapper 源码和 `FindCEF.cmake`，以及 GTK3、X11、Xi 开发包。CEF API 固定为 14900，并检查运行时 API hash。本机 CEF 152.0.6 提供该 API，不能仅凭 CEF 主版本不同就跳过 hash 检查。

```sh
cargo run -p xtask -- build --backend system-cef
dist/dnr --check-system-cef
ldd dist/dnr
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime -- --ignored
dist/dnr examples/desktop/smoke.ts
```

确认 `libcef.so` 来自 `/usr/lib/cef`，没有缺失依赖；GUI 测试退出后检查 Chromium 子进程正常退出。WebView 和 system-CEF 是两个构建变体，后一次构建替换 `dist/dnr`。

## 共同检查

- 从不同 cwd 直接执行同一个 `.dnp`，验证参数和资源路径。
- CLI 不依赖显示会话；普通 HTTP 服务不打开窗口。
- 包含托盘的应用在关掉最后一个窗口后仍可工作，移除托盘和后台任务后退出。
- 多个应用并行运行，不共享 JS 状态、文件句柄或应用存储身份。
- 检查发行目录仅需 dnr/dnc；没有生成或加载随附的 libdenort/Laufey 动态库。
- 系统 CEF 升级后重新执行 `--check-system-cef`，ABI 不匹配必须失败并重新构建，不能跳过校验。


## 生命周期与产物分离

补充桌面回归：

```sh
dist/dnr examples/desktop/tray-smoke.ts
# 应打印 DNR_TRAY_OK，退出码 0；期间只有托盘维持应用生命周期。
dist/dnr examples/desktop/exit-smoke.ts
# 预期退出码 7，窗口由宿主清理。
dist/dnr examples/desktop/exit-smoke.ts node
# 预期退出码 9。
dist/dnr examples/desktop/exit-smoke.ts error
# 预期退出码 1，并打印 DNR_EXPECTED_GUI_ERROR。
dist/dnr examples/desktop/native-close.ts
# 点击标题栏关闭按钮：即使 HTTP 服务仍在监听，也应退出 0，无需 Ctrl-C。
```

程序调用 `win.close()` 和系统关闭按钮必须分别验证。原生关闭事件中的同步 `Deno.exit()` / Node `process.exit()` 应立即进入宿主清理；没有显式退出时，仍应完成应用的后台任务。KDE Wayland 可使用以下脚本发送真实 compositor 关闭请求，只匹配测试启动的进程：

```sh
python3 scripts/test-linux-close.py --dnr dist/webview/dnr --repeat 3 \
  --logs dist/validation-linux/manual-close/webview
python3 scripts/test-linux-close.py --dnr dist/system-cef/dnr --repeat 3 \
  --logs dist/validation-linux/manual-close/system-cef
# 可加 --package /path/to/app.dnp，对未修改的应用包做相同验证。
```

脚本需要 `qdbus6`，测试期间使用临时 `XDG_DATA_HOME`，分别记录宿主退出和原生子进程清理耗时。WebKit 子进程在本机偶尔需要约 4.5 秒自行完成清理；测试最多等待 15 秒，超时视为失败。

CEF 浏览器进程在启动时设置 `--no-first-run` 与 `--no-default-browser-check`，不进入 Chrome 首次运行或默认浏览器确认流程。CEF Wayland 窗口仍需要 GTK3，为原生托盘、菜单及对话框初始化；普通 CLI 与 CEF 子进程不初始化 GTK 显示。托盘还需要系统 `libayatana-appindicator3.so.1` 或 `libappindicator3.so.1`；上游库的弃用提示不等同于测试失败。

每次构建会覆盖 `dist/dnr`。切换后端前保留已验证的工具：

```sh
mkdir -p dist/webview dist/system-cef
# WebView 构建并验证后：
cp dist/dnr dist/dnc dist/webview/
# system-CEF 构建并验证后：
cp dist/dnr dist/dnc dist/system-cef/
```

检查成功标记时必须同时检查退出码，避免将输出 `DNR_GUI_OK` 后的崩溃误记为通过。测试退出后还需检查对应二进制的浏览器/渲染子进程是否结束。本机日志保存在 `dist/validation-linux/`。
