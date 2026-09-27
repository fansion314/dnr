# 上游补丁登记表

基线：Deno `abd22074e47c6a5cd14e9e4e84743f084aa5a575`，
Laufey `1fe87874288e8359fa3de04d18cc14f56957b000`。
本表描述仓库内实际补丁，不表示这些修复已经提交或合入上游。
每次升级先对比这里列出的行为与上游实现，再删去已被等价修复的 hunk；不要仅按行号判断。

## 可由上游吸收的修复

| 标识 | 补丁与入口 | 原因和保留行为 | 回归 / 移除条件 |
| --- | --- | --- | --- |
| DENO-FS-EXCL | `deno.patch`: `ext/fs/interface.rs`, `OpenOptions::from(i32)` | 仅 `O_CREAT | O_EXCL` 映射为 `create_new`；单独 `O_EXCL` 保留在 `custom_flags`，不隐式加 `O_CREAT`，保留 Linux 块设备独占语义。权限判定、访问模式和其他标志不变。 | `examples/node-open-flags.mjs` 在 Node/dnr 对比 sync/callback/promise；`runtime_node_flags` 覆盖磁盘与 DNP。Linux 还需跟踪实际 open syscall，并在专用测试块设备上验证 EBUSY 和句柄寿命；普通文件成功不证明设备独占。上游同时保留系统标志且测试通过后删除。 |
| DENO-WINDOW-CANCEL | `deno.patch`: `cli/rt_desktop/lib.rs`, `cli/rt/desktop.rs` | 修复上游原生 close 通知不可取消的问题：原生回调只发送请求，JS 同步分发可取消事件，未取消才执行关闭动作。窗口被隐藏/取消时不更新销毁状态。 | `examples/desktop/close-api.ts`、`close-behavior.ts` 与真实原生关闭。上游提供等价协议且回归通过后可移除修复，不能同时误删下一项下游扩展。 |
| DNR-WINDOW-POLICY（新增扩展） | `deno.patch`: `cli/rt/desktop.rs`, `runtime/ops/desktop.rs`, `cli/tsc/dts/lib.deno.desktop.d.ts` | **本轮新增、尚未发布，非上游现有 API**：closeBehavior、get/setCloseBehavior、destroy；已有 close() 也改为分发可取消事件。`Deno_privateDesktopClose` Symbol 防止 fast-call 覆盖 JS 包装器。既有 `attachPanel().destroy()` 调用显式销毁，保留其无条件销毁语义。 | 上游基线及发布版 dnr 0.4.0 不支持新增接口；API 来源与迁移见 WINDOW-LIFECYCLE.md。取消原生事件的上游修复不自动替代策略扩展；删除/改名需兼容迁移。 |
| LAUFEY-TRAY-ACTIVATE | `laufey.patch`: `backend-common/src/tray_linux.cc` 及头文件说明 | 连接 AppIndicator 的 activate 信号，将左键送回 runtime。在 GTK 线程设置/替换/移除处理器，回调前释放注册表锁。按实际 GObject 信号探测能力，旧库提示降级，不因 ABI 名称相同就假设支持。 | Linux 原生托盘左键、右键菜单、处理器替换/移除和销毁；两个后端共享该实现。上游连接等价信号后删除。系统需要支持该信号的库，升级 Laufey 不能补齐旧系统库能力。 |
| DENO-READFILE-COPY | `deno.patch`: `ext/fs/ops.rs`, 同步/异步 readFile | 历史“二次字符串拷贝”记录对应这里的**字节缓冲区**复制：`buf.into_owned().to_vec().into()` 改为 `buf.into_owned().into()`。移动已拥有的数据；借用数据仍复制一次，不能直接暴露共享 ZIP 缓存为可写 JS 内存。 | `runtime::read_file_results_own_their_bytes` 覆盖 Deno/Node、多次读与并发结果不互相污染。上游去掉多余复制并保持所有权隔离后删除。关联上游原注释 [Deno #27107](https://github.com/denoland/deno/issues/27107)。 |
| DENO-FUNCTION-CACHE-KEY | `deno.patch`: `libs/core/ops_builtin_v8.rs` | 函数参数也参与 V8 code cache key；使用带长度的 UTF-16 参数序列，不能只比较 URL/函数体，也不改传入 V8 的实际源码。 | `runtime_cache` 中 CJS/worker/cache 行为；升级时确认不同参数不会复用错误的已编译函数。 |
| LAUFEY-CEF-EVAL-CLOSE | `laufey.patch`: `cef/src/{runtime_loader.cc,runtime_loader.h,app.cc,app.h}` | eval 回调按窗口归属保存；关闭时拒绝未完成请求；向 renderer 发 IPC 前检查 browser/frame 有效性。补齐 page-load 回调，退出时等待浏览器关闭后才结束 CEF 循环。 | 两个 Linux GUI 脚本中的关闭、异步收尾、退出无残留；需真实 CEF，WebView 结果不能替代。上游具备相同寿命保障再移除。 |

Ayatana 的 [activate 实现](https://github.com/AyatanaIndicators/libayatana-appindicator/blob/master/src/app-indicator.c)
在没有连接处理器时返回 `No handler for Activate`；因此此前错误并不一定表示系统库没有实现该方法。
协议背景见 [StatusNotifierItem](https://specifications.freedesktop.org/status-notifier-item/latest/status-notifier-item.html)。
本轮没有修改系统 AppIndicator 包，也没有替换 SNI 协议栈。

**LAUFEY-TRAY-ACTIVATE 的覆盖边界（2026-09-27）：** 当前目标不提供 Xfce 支持；
以下保留非目标环境的兼容性记录。xjtuse 的 Xfce 会话实际鼠标左键仍打开菜单，
虽然直接 D-Bus Activate 已成功进入回调。Ayatana 0.6.0 的协议缺少 `ItemIsMenu`，
[Xfce 源码](https://github.com/xfce-mirror/xfce4-panel/blob/master/plugins/systray/sn-item.c)
将缺省值设为 true。这个组合需要进一步的系统库/协议适配，当前不是完整桌面左键恢复修复。
不要为让测试通过而修改用户面板设置、把右键/菜单点击冒充左键，或把系统库补丁写成已经包含在 dnr 里。

随后在用户确认已切换完成的 Plasma/KWin X11 6.7.5 会话中，同一 Ayatana 0.6.0 的
松间 DNP 已通过真实鼠标左键恢复、页面实例保留和右键菜单退出。该补丁在 KDE 的实际
点击路径已验证，Xfce 的兼容默认值限制仍保留，不能把两者混写为全部 Linux 桌面通过。

DENO-FS-EXCL 另在 `ext/fs/interface.rs` 添加标志分离单元测试，在 `ext/fs/std_fs.rs`
添加真实 Unix 文件打开回归；仅后者的测试代码有改动，生产打开函数保持上游实现。

## dnr 专属接入（不能因为升级上游而整块删除）

| 范围 | 持久维护位置 / 上游补丁入口 |
| --- | --- |
| 静态 runtime/后端和平台构建 | `integration/rt/build.rs`、`integration/native/*`；两个 patch 的 Cargo、runtime loader、平台 main 接入 |
| DNP/VFS、Node-API/FFI、原生子进程 | `crates/package/*`、`integration/rt/dnr_vfs.rs`；Deno `file_system.rs`、`node.rs`、`ext/process/lib.rs` 的宿主入口 |
| 应用身份、离线解析、TS/JS 加载、顶层 await | `integration/rt/dnr.rs`；Deno `binary.rs`、`run.rs`、`cli/lib/worker.rs`。加载器修复随相关原生 runtime 测试复验 |
| V8/转译缓存与 Worker 继承 | `integration/rt/dnr_cache.rs`；Deno `runtime/{code_cache,worker,web_worker}.rs`。保留读取、晚写入和 worker 回调路径 |
| 懒 GUI 初始化和窗口/托盘/任务保活 | `integration/rt/desktop_tail.rs`；Laufey `capi/src/lib.rs`、WebView/CEF 平台事件循环。关闭最后一窗不能直接终止后台服务 |
| macOS Dock 重开 | Laufey `webview/src/main_mac.mm` 与 Deno desktop 类型说明；既派发应用事件，也保留 AppKit 的默认恢复 |

`dnr_desktop.rs` 由 xtask 从 patched `cli/rt_desktop/lib.rs` 与本地 tail 生成。
修改生成文件不构成可维护修复，必须保留上游 patch 或本地 integration 源文件。

## 升级与复验

1. `python3 scripts/check-upstream-patches.py` 在独立 Git 快照检查应用/反向，不修改 mirror。
2. 候选上游版本逐项对比上表。已修复项记录新提交和相同回归结果，再移除对应 hunk。
3. 保存生成树中必要调试修改，从干净快照 `xtask prepare`；不能把新 patch 叠到旧 patch 上。
4. 构建真实 runtime；普通 workspace 测试不能覆盖内嵌 Deno。执行显式原生测试与平台 GUI 测试。
5. 把实际平台/后端、未覆盖范围写入 `VALIDATION.md`。本表不替代验证记录。
