# Linux x86_64 验收

根据用户要求，Linux 原生构建、Wayland 和系统 CEF 验收留待用户执行；本轮没有在 Linux 安装依赖或进行构建。

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

在真实 Wayland/NVIDIA 环境中复验。dnr 在初始化后端之前为 Wayland 会话设置 `__NV_DISABLE_EXPLICIT_SYNC=1`，但本轮未验证具体驱动和 compositor 的效果。

## 系统 CEF

首版针对 Songjian 使用的 Arch/CachyOS 布局。系统需提供 `/usr/lib/cef`、CEF headers、wrapper 源码和 `FindCEF.cmake`，以及 GTK3、X11、Xi 开发包。CEF API 固定为 14900，并检查运行时 API hash。

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
