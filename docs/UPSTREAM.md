# 跟踪上游与补丁边界

固定源版本由 `integration/upstreams.json` 记录：Deno `fe7ed012d9`、
Laufey `1fe8787`（Deno v2.9.8）。xtask、补丁检查和 CI 使用其中的完整提交号。

## 实现放在哪里

| 行为 | 维护位置 | 上游保留的接入点 |
| --- | --- | --- |
| v4 元数据、文件校验、原生分组、持久缓存 | `crates/package/src/{metadata,index,materialize,persistent}.rs` | 不依赖 Deno |
| ZIP 覆盖、包路径归一化、原生库加载 | `integration/rt/dnr_vfs.rs` | `file_system.rs` 的字段与 trait 方法入口 |
| V8/转译持久缓存 | `integration/rt/dnr_cache.rs` | loader 选择适配器、转译入口与 Worker 服务传递 |
| 应用身份、进程启动、安装目录 | `integration/rt/dnr.rs` | runtime 模块注册和运行入口 |
| GUI 与主线程生命周期 | `integration/rt/desktop_tail.rs`、`dnr_desktop.rs` 生成流程 | Laufey 后端与 worker 生命周期钩子 |

`dnr_vfs.rs` 作为上游 `file_system` 的子模块编译，以访问必要私有状态；不把上游字段改成公共 API，也不复制整个上游文件。新增文件由 xtask 同步，修改必须保存在 `integration/rt/`。

`RuntimeCodeCache` 在本地组合 dnr 持久后端与未修改的 Deno standalone 后端，保持晚加载持续写入。`cli/rt/code_cache.rs` 不再带 dnr 分支。

深层 V8 API 性能探针已撤回，`DNR_PROFILE` 仅保留本地转译和写入诊断。`libs/core/ops_builtin_v8.rs` 中函数参数参与缓存键的修复仍保留：同 URL、同函数体但参数不同必须区分。源码读取不必要复制的修复、顶层 await 判定、Worker 缓存回调及原生子进程解析也保留，不能为了补丁行数删除行为保障。

文件系统 trait 入口仍需将组的真实路径映射回 ZIP 逻辑路径，包括写操作的只读检查。仅在读取入口归一化会让真实缓存路径绕过只读语义；这部分重复接入有明确用途。

## 在不改动构建树的情况下检查

```sh
python3 scripts/check-upstream-patches.py
# 若本地 mirror 没有固定提交，指定包含该提交的独立检出。
python3 scripts/check-upstream-patches.py --deno /path/to/deno --laufey /path/to/laufey
# 先只读检查候选提交；不切换 mirror 分支，不更新项目的固定版本。
python3 scripts/check-upstream-patches.py --deno-revision <候选提交> --laufey-revision <候选提交>
```

脚本从给定仓库的 Git 对象读取补丁涉及的文件，在临时目录执行正向检查、实际应用和反向检查，并报告文件数、hunk 数和增删行。它不会复制未提交的 mirror 改动，不会修改 mirror、`.upstream` 或 Git 索引。通过只表示补丁可应用，不代表 API/ABI 或运行时兼容。

`xtask prepare` 从固定提交归档两个上游，先在临时目录应用补丁，再发布准备树。
已有过期准备树会移到 `.upstream/<name>.backup-*`，调试改动可从备份恢复。
每棵准备树的 `.dnr-provenance.json` 记录上游提交、补丁摘要和源码文件清单；
`xtask build` 在同步接入代码和构建 dnc 前校验完整清单。接入源码与锁文件仍由
`integration/` 和 xtask 同步管理。备份确认不再需要后才手工清理。

## 升级步骤

1. 在独立检出中检查上游 API 变化；先运行候选补丁检查，逐项确认冲突对应的宿主行为。
2. 优先将宿主策略放进本地模块，只保留必要调用入口；修复上游通用问题的补丁应单独说明原因，待上游修复后删除。
3. 保存已有生成树调试改动，再用新版本干净检出执行 `xtask prepare`；旧准备树有旧补丁时不能直接覆盖套用。同步固定版本、两份锁文件、snapshot 和 CEF ABI 设置。
4. 保留默认 sccache，运行工作区 fmt/clippy/tests、完整 release 构建和显式 runtime/native/groups/cache 测试；补充实际 Pi 和 GUI 验证。
5. macOS 与 Linux 分开记录。macOS 成功不代表 Linux WebView 或 system-CEF 已验证；补丁检查也不能代替原生验收。

补丁数量以检查脚本的当前输出为准。逐项维护原因、验证入口和上游吸收条件见
[补丁登记表](PATCHES.md)，窗口关闭策略见 [桌面窗口生命周期](WINDOW-LIFECYCLE.md)。
