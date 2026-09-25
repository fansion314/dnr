# v0.3.0 验证与 Pi 启动实测：2026-09-25

以下记录对应重构前的 `5cfb906` 实现。随后已移除 v1/v2 支持和上游 V8 深层计时探针；本文数据、二进制摘要与五组基准保留为历史证据，复现应使用该提交。当前上游维护边界见 [UPSTREAM.md](UPSTREAM.md)，新一轮验收见根目录 [VALIDATION.md](../VALIDATION.md)。

在本机 macOS ARM64 上，同一 v3 Pi 包关闭编译缓存的启动中位数为 **252.33 ms**，暖缓存为 **208.40 ms**，减少 **43.94 ms（17.4%）**。首次填充为 **324.09 ms**，因此首次启动存在额外成本。

## 输入与方法

- 环境：macOS-27.0-arm64-arm-64bit-Mach-O；macOS ARM64 / WebView，Deno 2.9.7、Laufey 0.7.0；未升级固定上游。
- Pi 0.87.1：从当前安装包导出同一份逻辑内容，再分别打成 v2/v3；比较全部文件摘要、权限、分组、平台和入口，不以包总大小代替内容一致性。
- cwd：`/Users/peilin/Codebase/dnr`。沿用当前用户配置与扩展；不使用空配置替代正式 help 测量。
- 每组预热 5 次，正式 30 次；5 组按固定 seed 20260925 随机交替，共 150 个正式进程。保留全部离群值。
- 基准通过 dnr 直接执行包及 `--help`，不包含 shell 启动头的额外进程。OS 文件缓存经过预热；“冷”仅指新的 dnr 持久缓存。
- 缓存分别放进验证目录；每个首次填充样本使用新的缓存目录。正式测量关闭 `DNR_PROFILE` 和缓存统计日志。
- 所有输出均为 10,176 字节，SHA-256 `5668cff3cf34fd6f0dbcb7b0d10667761219ca47879ba09d11c137507245824c`，退出 0。

## 端到端结果

| 组别 | 平均 ms | 中位数 ms | 标准差 ms |
| --- | ---: | ---: | ---: |
| v2，原有无编译缓存行为 | 251.16 | 252.61 | 10.65 |
| v3，关闭两层编译缓存 | 252.84 | 252.33 | 16.22 |
| v3，仅转译缓存 | 251.27 | 252.86 | 9.07 |
| v3，完整暖缓存 | 211.76 | 208.40 | 19.17 |
| v3，首次填充 | 326.36 | 324.09 | 17.65 |

暖缓存诊断记录为 **214 次命中、0 次未命中**。这份 Pi 主体已是 JS，当前 help 路径没有经过 dnr AST 转译入口，因此仅开启转译缓存没有显示出收益；不能将其泛化到未打包的 TS 应用。
暖缓存逻辑文件大小 3,644,824 字节，按 inode 去重的文件分配空间 4,104,192 字节（不包含目录分配）。不是应用运行内存。

## 独立耗时探针

以下是在同一最终二进制上单独开启 `DNR_PROFILE` 的 5 次采样中位数。V8 API 总数按每次样本先求和再取中位数；不把不同子项中位数相加拼成端到端时间。日志输出不在对应计时区间内，但探针会扰动整个进程，因此本表不用于替代上表。

| 调用边界 | 关闭缓存 ms | 暖缓存 ms | 首次填充 ms |
| --- | ---: | ---: | ---: |
| V8 显式编译／反序列化 API 总计 | 71.54 | 13.60 | 74.19 |
| 其中 ESM API | 48.21 | 8.81 | 49.51 |
| 其中 CompileFunction API | 22.90 | 4.29 | 24.24 |
| 其中 Script API | 0.43 | 0.43 | 0.43 |
| V8 code cache 序列化 | 0.00 | 0.00 | 17.30 |
| 字节码缓存发布调用 | 0.00 | 0.00 | 45.39 |
| AST 转译 | 0.00 | 0.00 | 0.00 |
| 转译缓存发布调用 | 0.00 | 0.00 | 0.00 |

缓存发布计时包含封装、摘要、文件操作及索引入队，不是纯内核 I/O 时间。SQLite 异步工作不包含在调用计时里。V8 API 探针不覆盖执行期间所有内部惰性编译，也不测应用顶层初始化。首次填充同时承担编译、序列化和缓存发布成本。

## 二进制元数据与包大小

- 独立 `open_performance` release 微基准：每种格式预热 10 次、正式 100 次，交替顺序；不读取应用 payload。v2 中位数 **1.68 ms**，最终 v3 解码实现 **1.53 ms**。这是独立 package 库微基准，不是完整 runtime 的阶段计时。
- 初版 v3 曾因逐字节格式化摘要造成大量临时分配而更慢；修复为一次分配的十六进制转换，并避免重复计算 metadata SHA-256 后才得到上述结果。
- v2 包 6,653,596 字节，v3 包 6,699,785 字节：本例增加 46,189 字节。Stored 元数据以少量体积换取直接读取，不能宣称包更小。
- v3 原生变体采用短物理路径；含相对链接的组保留必要结构并去掉公共逻辑前缀。原生缓存/硬链接用功能测试验收，本 help 基准不代表原生组解压或 GUI 加速。

## 功能与平台验收

- 工作区 fmt、clippy（warnings 为错误）、普通测试通过；普通测试不代替忽略的 native 测试。
- 最终 macOS release：19 项 runtime/原生测试通过，包括 v1/v2 兼容、v3 Node-API/FFI/外部进程、完整解包后的原生加载、CJS、Web/Node Workers、TS 扩展同大小同 mtime 变更、晚加载、缓存损坏、不可用缓存和四进程冷启动。
- v3 包层测试覆盖确定性身份、Stored 元数据、短路径、相对符号链接、原生硬链接、旁置优先、完整安装占用锁、迟到旧代不回滚当前、旧运行时缓存回收、崩溃暂存回收和管理命令 dry-run。
- Pi 的结构测试及 cache/sidecar 两种部署的离线烟雾通过：真实 Node-API、Photon WASM、TS 扩展、faux 模型回复、bash、会话与 HTML 导出；`npm run check` 通过。
- 隔离 PTY 中冷/暖两次收到 `DNR_V3_INTERACTIVE_OK`，Ctrl+D 退出码均为 0；使用离线 faux provider，没有付费模型调用。此检查不是 30 次交互首帧基准。
- 真实 macOS WebView v3 包输出 `DNR_GUI_OK` 并退出 0；原生窗口脚本通过隐藏、Dock 恢复、最小化和标题栏关闭的两轮回归。
- 最终 Deno 补丁在固定 mirror 文件的独立干净副本上正向应用并逐文件匹配，20 个文件通过；未修改 mirror。
- **Linux x86_64 / system-CEF 本轮未验证**，已由用户明确确认保留此状态。只读检查 yama-ts（Debian 12）时，pkg-config 找不到 GTK3/WebKitGTK 4.1，且没有 `/usr/lib/cef/libcef.so`；没有安装依赖或修改远端项目。
- 产物只更新仓库 `dist/`；没有替换系统已安装的 dnr/Pi，没有提交、推送或发布。

## 产物与复现

- `dist/dnr` SHA-256：`eecbf80c0450927f799d366d44ed76e7dfd7ef99a45ec099c81959d3ebcb70be`。
- `dist/dnc` SHA-256：`b74fff4a575eadee138b462944d00bcb9aa9f2f4b74d1a92e1a5d5769ee98b69`。
- v2 输入 SHA-256：`c5c4fa45d91e8d6f5e2e518cf46fe23014347232f8305b23da2e9451339debf7`。
- v3 输入 SHA-256：`d7508ae08a2ffa8ffbec28fa5e26d6d79bb41abb56fd3af25dea5c0c146dea50`。
- 原始数据：`dist/validation-v0.3.0-20260925/startup-final/results.json`、`profile/results.json`、逐次探针日志和 `interactive/`。初轮对照保留在 `startup/`，不能混入最终样本。
- 输入副本保存在该验证目录的 `inputs/`。正式采样位置为 `/private/tmp/dnr-v3-pi/`；换路径会生成不同的路径缓存，复现时重新预热。

```sh
CARGO_CACHE_RUSTC_INFO=0 cargo run -p xtask -- build
CARGO_CACHE_RUSTC_INFO=0 cargo test --workspace
CARGO_CACHE_RUSTC_INFO=0 DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package \
  --test runtime --test runtime_native --test runtime_groups --test runtime_cache -- --ignored
python3 scripts/bench-startup-cache.py --dnr dist/dnr --dnc dist/dnc \
  --v2 dist/validation-v0.3.0-20260925/inputs/pi-v2.dnp \
  --v3 dist/validation-v0.3.0-20260925/inputs/pi-v3.dnp \
  --output dist/validation-v3-recheck --runs 30 --warmup 5
```

按宿主 RTK 约定执行上述命令时添加 `rtk` / `rtk proxy`。保留默认 sccache；没有通过清空 `RUSTC_WRAPPER` 绕过。
