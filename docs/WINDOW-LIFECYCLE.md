# 窗口关闭、隐藏与销毁

## API 来源与兼容性

**以下关闭策略 API 是 dnr 0.4.1 新增的下游扩展，
不是当前固定 Deno Desktop（`abd22074e4`）自带的 API，也未提交或合入上游。**
dnr 0.4.0 不包含这些接口；需要 dnr 0.4.1 或更新的兼容版本。

| 接口 / 行为 | 上游基线与本轮改动 |
| --- | --- |
| `BrowserWindowOptions.closeBehavior` | dnr 新增，取值 `"hide"` / `"destroy"`，v0.5.0 默认 `"hide"` |
| `setCloseBehavior()` / `getCloseBehavior()` | dnr 新增，设置 / 查询关闭策略 |
| `destroy()` | dnr 新增，跳过 close 事件；保留原有 WebGPU 句柄保护 |
| 原生 `close` 事件可取消 | 修复上游已有事件：使 preventDefault 有效，原生回调等待 JS 决策 |
| `close()` | 上游已有方法，**行为变化**：本轮开始同步分发可取消 close，再执行当前策略；上游原方法直接关闭 |
| `hide()` / `show()` / `focus()` / `isClosed()` | 上游已有方法，签名未变；隐藏不将窗口标记为已销毁 |

既有 `tray.attachPanel().destroy()` 使用显式销毁入口，继续保持无条件销毁面板的语义。

v0.5.0 增加逻辑窗口与原生实例分离，Laufey 下游 C ABI 36 提供真实窗口存在状态及普通尺寸/屏幕工作区查询。
可取消协议修复与策略扩展分别记录在 `PATCHES.md`；未来上游修复事件后，
不能把 dnr 新增策略 API 连同修复一起删除。

需要兼容旧 runtime 的应用可以检查 `typeof win.setCloseBehavior === "function"`。
旧 runtime 可能忽略未知构造参数，不能通过“构造没报错”判断策略生效。

## 使用方式

在支持该扩展的 dnr 中：

```ts
const win = new Deno.BrowserWindow({ closeBehavior: "hide" });
win.setCloseBehavior("destroy"); // 也可在运行中切换
console.log(win.getCloseBehavior());
win.close();   // 分发可取消的 close，然后执行当前策略
win.destroy(); // 跳过 close 事件，显式销毁
```

## v0.5.0 隐藏后的内存策略

**行为变化：** 未配置的窗口现在默认关闭为隐藏，并在隐藏 5 分钟后释放原生窗口和页面。
需要继续关闭即永久销毁的应用显式设置 `closeBehavior: "destroy"`。
需要长期保留 DOM、页面计时器和输入内容的应用设置 `hiddenWindowPolicy: { mode: "keep" }`。

```ts
const win = new Deno.BrowserWindow({
  closeBehavior: "hide",
  hiddenWindowPolicy: { mode: "delayed", delayMs: 300_000 },
});
win.setHiddenWindowPolicy({ mode: "immediate" });
win.setHiddenWindowPolicy({ mode: "keep" });
win.setHiddenWindowPolicy({ mode: "delayed", delayMs: 60_000 });
console.log(win.getHiddenWindowPolicy());
```

策略适用于原生关闭、`close()` 和 `hide()`；最小化不触发。重复 hide 不重置同一次隐藏计时。
`show()` / `focus()` 取消待释放任务；已经释放时自动重建，关闭尚未完成时排队恢复。
CEF 必须实际完成原生关闭才进入已释放状态，不用发出关闭请求代替确认。

释放保留 JS `BrowserWindow` 对象、稳定 windowId、宿主事件监听、bindings、窗口配置和
最近一次宿主 `navigate()` 入口；重建先安装绑定再导航，页面重新触发 load。
DOM、页面 JS 内存、历史栈和未保存输入不恢复。已经释放时 `executeJs()` 拒绝，不隐式重开；
释放时未完成的页面调用失败。标题、尺寸等配置变更保存到描述中，供下次重建使用。

`destroy()` 始终永久销毁、跳过 close 事件；释放状态的 `isClosed()` 仍为 false，永久销毁后为 true。
永久销毁后的 show 不重建。两种行为均不卸载整个后端，也不保证共享浏览器进程立即退出。
WebGPU 已导出原生句柄时保留上游保护，跳过释放并警告，防止悬空句柄。

原生关闭按钮和 `close()` 走同一条路径：

1. 原生后端延迟关闭，把请求送到 JS。
2. 同步分发 `new Event("close", { cancelable: true })`。
3. 未取消时执行当前 `closeBehavior`；默认动作进入隐藏/释放或永久销毁状态；旧原生实例的迟到事件不会派发给重建后的页面。

```ts
win.addEventListener("close", (event) => {
  event.preventDefault(); // 必须在同步事件处理阶段取消
  win.hide();
});
```

异步确认应先同步 `preventDefault()`，等待确认后调用 `destroy()`；不要在 `await` 后才取消。
事件处理器中重入 `close()` 不会递归分发。已销毁窗口的 `close()` / `destroy()` 幂等。
迁移注意：旧代码若依赖 `close()` 无条件关闭，或在取消事件后再次调用 `close()` 来强制销毁，
应明确改用新增的 `destroy()`。事件处理器应自行保证多次关闭请求下的业务逻辑幂等。
`destroy()` 保留上游 WebGPU 原生句柄保护：已取得 WebGPU surface 时仍降级为隐藏，
因此此情况下不能假设 `isClosed()` 为 true。

隐藏且尚未释放的窗口仍计入生命周期；已经释放的逻辑窗口不单独保活，无托盘或后台任务时允许自然退出。托盘/窗口/后台服务应由应用统一安排退出，例如托盘退出项
调用 `Deno.exit(0)`。没有托盘、重开入口或其他恢复方式时，不宜选择隐藏策略。

Linux 左键回调要求系统加载的 AppIndicator 实现提供 `activate` 信号
（Ayatana 0.6 系列）。Laufey 运行时探测信号并连接；旧版/legacy 实现保留托盘菜单，
设置左键处理器时输出明确提示。桌面面板自身的点击策略也会影响事件是否发送。
双击不是 StatusNotifier 的标准事件，本轮没有伪造双击支持。

**目标范围：** 当前 Linux 桌面验收以 KDE/Plasma 为目标，不提供 Xfce 支持。
以下 Xfce 记录保留为兼容性边界，不是本轮交付阻塞项。

**非目标环境记录：** xjtuse 的 Xfce 4.20.8 + Ayatana 0.6.0 实测，直接 D-Bus `Activate`
能触发回调，但实际左键仍打开菜单。Ayatana 没有发布 `ItemIsMenu`，Xfce 将缺省值当作
true，因此不会发送 Activate。本补丁解决空回调/`No handler for Activate`，并没有补齐
系统库的这个协议属性；不能把该组合称为左键恢复已通过。

同日改在同一测试机的 **Plasma/KWin X11 6.7.5** 上，保持 Ayatana 0.6.0 不变，
松间 DNP 的真实鼠标左键恢复已通过，并确认窗口 ID、页面 token、加载次数不变。
这证明该面板能在当前属性缺失的情况下发出激活请求；不能推广成其他版本、Wayland
或其他桌面也已验证。如将来扩展到 Xfce，需另行适配。详见 `VALIDATION.md`。

回归入口：`examples/desktop/close-api.ts` 自动检查取消、重复调用、隐藏/恢复同一页面、
切换策略和显式销毁；`close-behavior.ts <icon.png>` 用真实窗口关闭按钮与托盘交互复验。
使用 `--backend webview` 和 `--backend system-cef` 分别运行，不能互相代替。
