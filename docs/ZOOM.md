# 桌面内容缩放

这是 dnr 0.4.2 新增的下游扩展；0.4.3 增加应用倍率自动持久化。
v0.5.0 将缩放与窗口尺寸统一为默认开启的应用状态记忆，使用 5 秒防抖保存。
0.4.1 和固定上游 Deno Desktop 不包含本接口。

## 用户统一设置

```sh
dnr zoom             # 查看已保存的倍率和配置路径
dnr zoom set 1.25    # 125%；重新启动应用后生效
dnr zoom reset       # 恢复 100%
```

全局倍率和应用倍率均接受有限数字 0.5–2.0。实际页面倍率是两者乘积，范围 0.25–4.0。
系统 DPI / 显示器缩放由后端继续处理，不能把 devicePixelRatio 再乘到页面倍率上。
本功能调整页面内容，不修改原生窗口边框、菜单或窗口逻辑尺寸，也不根据分辨率猜测倍率。

专用配置为 `zoom.json`（`{"version":1,"zoomFactor":1.25}`），目录选择：

- `DNR_CONFIG_DIR`：显式指定绝对目录，优先于平台默认值。
- macOS：`~/Library/Application Support/dnr`。
- Linux：`$XDG_CONFIG_HOME/dnr`，未设置时为 `~/.config/dnr`。

查询不创建配置文件；缺少文件为 1。写入使用同目录临时文件和原子替换。
配置损坏或读取失败时，查询报错；启动应用警告一次并使用 1。`set/reset` 可以修复配置。
应用启动、执行用户代码前读取一次；之后更改配置不影响该进程，包括其后来创建的窗口。
设置不写入 DNP，不跟随应用发行，也不放入可清理的缓存。脚本名为 zoom 时使用 `dnr ./zoom`。

## 应用接口

```ts
Deno.desktop.setZoomFactor(1.2);
console.log(Deno.desktop.getGlobalZoomFactor());    // 例如 1.25
console.log(Deno.desktop.getZoomFactor());          // 1.2
console.log(Deno.desktop.getEffectiveZoomFactor()); // 1.5

Deno.desktop.addEventListener("zoomchange", (event) => {
  console.log(event.detail); // globalFactor, appFactor, effectiveFactor, source
});
```

应用倍率首次使用默认为 1，**当前应用进程的所有应用窗口共享**。API 和快捷键修改
同一份应用倍率，dnr 自动保存，并在下次启动、执行应用代码前恢复。应用无需自行保存；
启动时主动调用 `setZoomFactor` 仍会覆盖已恢复的设置，因此不要为了设置默认值而每次启动
无条件调用 setter。全局倍率只能通过命令配置，不被应用 API 覆盖。
CEF 站点缩放共享不需要改变 Cookie/localStorage 上下文。

应用倍率与窗口尺寸写入配置目录的 `app-state/dnr-<SHA-256>.json`，例如：
`{"version":1,"zoomFactor":1.25,"windows":{"main":[880,600]}}`。
首次读取兼容旧 `app-zoom` 文件；下一次设置保存时导入，不改写原文件。
身份沿用应用存储身份：DNP 和完整安装目录使用 manifest 的 appId，升级、移动包或切换
DNP/完整安装方式后保留；裸脚本使用规范化入口绝对路径，不同脚本隔离。
保存的是第三层应用倍率，系统 DPI 和全局倍率不写入该值。`Cmd/Ctrl+0` 或
`setZoomFactor(1)` 恢复并保存应用倍率 1；`dnr zoom reset` 只重置全局倍率。

设置变化先更新内存，缩放和尺寸共用最后一次变化后的 **5 秒**防抖队列，合并原子写入。
正常退出及 `Deno.exit()` / `process.exit()` 补写；强制终止可能丢失最后几秒的修改。
无变化不写入。不同应用使用不同文件；同应用多进程持有各自启动快照，保存时加锁合并
本进程修改的字段，不用旧尺寸覆盖其他进程的倍率，也不向运行中的进程广播。
损坏的应用文件警告并使用默认值，下一次修改修复；写入失败只警告，不影响内存中的设置。
快捷键启用状态不持久化。

```ts
// 在创建窗口前调用；两项默认都是 true，参数可只指定其中一项。
Deno.desktop.setStatePersistence({ zoom: false, windowSize: false });
console.log(Deno.desktop.getStatePersistence());
const main = new Deno.BrowserWindow({ stateKey: "main", rememberSize: true });
```

关闭记忆会停止恢复和后续保存，并取消该项待保存修改，不删除历史记录。
关闭缩放记忆时，尚未被本次操作覆盖的启动恢复值回到 1；关闭尺寸记忆不改变已打开窗口。
重新开启不会突然调整现有窗口，后续新窗口可恢复已保存尺寸。开关只影响本次进程。

首个普通窗口默认使用 `main`，其他窗口需显式 `stateKey`；托盘面板和开发者工具不占用默认身份。
活动窗口的 key 不允许重复；`rememberSize: false` 可关闭单个窗口的尺寸记忆。
仅保存普通状态的内容区逻辑宽高，不保存位置、最大化或全屏状态；恢复尺寸限制到屏幕可用区域。
恢复值优先于构造参数中的默认宽高，后续显式 `setSize()` 可以覆盖。

这些 API 在创建窗口前也可调用，不初始化 GUI。setter 更新控制器状态并安排原生 UI 操作；
getter 返回控制器状态，不保证屏幕已同步重绘。`zoomchange` 异步派发，source 为 `api` 或
`shortcut`；相同值不重复派发。非数字抛 TypeError，非有限值或越界抛 RangeError，旧状态保留。
页面需要通过应用自行注册的 bindings 使用接口，不自动向网页开放新的宿主能力。

## 快捷键

macOS 使用 Cmd，Linux 使用 Ctrl：`+` / `=` 放大、`-` 缩小、`0` 恢复应用倍率 1。
重置后仍保留全局倍率。支持数字键盘，忽略长按重复；每次选择相邻档位：

`0.5, 0.67, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2`。

```ts
Deno.desktop.setZoomShortcutsEnabled(false); // 应用自行管理按键
console.log(Deno.desktop.getZoomShortcutsEnabled());
```

开关默认为 true；必须传 boolean。启用时快捷键在应用窗口的原生事件路径消费，避免页面或
CEF 再处理一次；关闭平台快捷键时按键交给页面/应用，但仍抑制 Chromium 自带的缩放动作，以免实际页面倍率绕过控制器。开发者工具不受平台快捷键管理。不接管滚轮或触控板手势。

## 维护与验证

Rust 控制器保存两层倍率；Laufey 下游 C ABI 36 包含页面倍率、快捷键回调和窗口状态查询，不能与未修改的
上游 API 34 后端混用。三个后端使用 WKWebView pageZoom、WebKitGTK zoom-level 和
CEF 的对数 zoom level，不注入页面 CSS、不强制修改系统 device scale。

`runtime_zoom` 覆盖真实 runtime 的配置、API、事件、进程快照、立即退出后的恢复、
裸脚本/包身份隔离、包移动与完整安装共享设置，以及无显示路径；
`examples/desktop/zoom.ts` 验证多窗口的实际 CSS 视口、导航、隐藏恢复和新窗口。
加 `--interactive` 后保留窗口并输出仅监听 loopback 的测试控制端点，供真实键盘验证。
Linux 真实快捷键回归用 `GUI_SESSION_SLOT=verify gui-run python3 scripts/test-linux-zoom.py --backend webview --logs .cache/validation/zoom/webview`；CEF 将后端改为 `system-cef`。可选 `--data` 使用 data URL 做导航故障对照，不代表 HTTP/跨站点验收。
原生快捷键匹配单元测试为 `integration/native/tests/zoom_shortcuts.cc`。
`python3 scripts/test-zoom-persistence.py` 验证 macOS Cmd+= 后重启恢复、API 设置后重启恢复、
reset 后重启恢复，并读取真实页面视口。Linux 使用
`GUI_SESSION_SLOT=verify gui-run python3 scripts/test-zoom-persistence.py --backend webview`，
CEF 改为 `--backend system-cef`；隔离会话的首次 KWallet 向导可加 `--cancel-wallet-setup`。
实际执行结果和未覆盖环境见 [VALIDATION.md](../VALIDATION.md)。

系统 CEF 152 的实测中，即使设置 `chrome_zoom_bubble=STATE_DISABLED`，仍可能显示原生缩放提示。
dnr 也接管其放大、缩小和 Reset 命令，通过同一应用倍率控制器执行；Reset 不会清除全局倍率。
提示是否隐藏取决于系统 CEF，本版本不声称所有系统库均能隐藏该提示。

独立 KDE 验证会话第一次启动 CEF 时，KWallet 初始化向导可能阻塞 HTTP 加载。
测试脚本的可选 `--cancel-wallet-setup` 仅允许在 `GUI_SESSION_SLOT=verify` 中使用，根据实时
控件树取消该向导；不创建钱包、不修改密码存储配置。普通桌面运行不自动操作该对话框。
