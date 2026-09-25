# 跟踪上游与补丁边界

固定源版本仍为 Deno `abd22074e4`、Laufey `1fe8787`。本次重构不升级依赖或发布版本。

## 实现放在哪里

| 行为 | 维护位置 | 上游保留的接入点 |
| --- | --- | --- |
| v3 元数据、文件校验、原生分组、持久缓存 | `crates/package/src/{v3,index,materialize,persistent}.rs` | 不依赖 Deno |
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
# 先只读检查候选提交；不切换 mirror 分支，不更新项目的固定版本。
python3 scripts/check-upstream-patches.py --deno-revision <候选提交> --laufey-revision <候选提交>
```

脚本从给定仓库的 Git 对象读取补丁涉及的文件，在临时目录执行正向检查、实际应用和反向检查，并报告文件数、hunk 数和增删行。它不会复制未提交的 mirror 改动，不会修改 mirror、`.upstream` 或 Git 索引。通过只表示补丁可应用，不代表 API/ABI 或运行时兼容。

## 升级步骤

1. 在独立检出中检查上游 API 变化；先运行候选补丁检查，逐项确认冲突对应的宿主行为。
2. 优先将宿主策略放进本地模块，只保留必要调用入口；修复上游通用问题的补丁应单独说明原因，待上游修复后删除。
3. 保存已有生成树调试改动，再用新版本干净检出执行 `xtask prepare`；旧准备树有旧补丁时不能直接覆盖套用。同步固定版本、两份锁文件、snapshot 和 CEF ABI 设置。
4. 保留默认 sccache，运行工作区 fmt/clippy/tests、完整 release 构建和显式 runtime/native/groups/cache 测试；补充实际 Pi 和 GUI 验证。
5. macOS 与 Linux 分开记录。macOS 成功不代表 Linux WebView 或 system-CEF 已验证；补丁检查也不能代替原生验收。

本轮 Deno 补丁由 20 文件、128 hunks、`+509/-98` 收敛至 15 文件、110 hunks、`+394/-79`。Laufey 保持 10 文件、27 hunks、`+165/-33`。统计只计补丁正文，不把迁入本地模块的行数当成逻辑删除。
