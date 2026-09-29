# v4 用户缓存和安装

当前 dnr 默认对 v4 包、包内代码和包外扩展启用 V8 code cache 与 TS/TSX/JSX
转译缓存。v1/v2/v3 包不再支持；普通磁盘脚本不使用持久编译缓存。缓存减少编译工作，不保存应用运行状态，
不跳过顶层初始化，也不取代源码。完整解包安装通过 `dnr <安装目录>` 保留同样能力。

## 身份与目录

用户缓存继续使用 `DNR_CACHE_DIR`，未设置时为 macOS `~/Library/Caches/dnr` 或
Linux `${XDG_CACHE_HOME:-$HOME/.cache}/dnr`。应用数据仍按 appId 隔离，不属于缓存。

```text
<cache>/
  index.sqlite3                    # 可重建的异步索引
  v4/<pathHash>/
    current                        # 当前 contentHash
    .gate
    .leases/<contentHash>
    generations/<contentHash>/
      receipt.json
      native/<target>/<group>/root/<logical-path>
      compile/
        current
        .leases/<fingerprint>
        generations/<fingerprint>/{emit,code}/<module-key>
```

`pathHash` 是规范化真实绝对路径的 SHA-256；符号链接入口归并到其目标。完整安装以
目录真实路径标识。`contentHash` 来自 dnc 的二进制元数据，其文件摘要覆盖所有平台。
首次读取应用文件仍验证 ZIP 尺寸、CRC、SHA-256；内容身份不能替代这些校验。

同路径更新只设一个当前代。运行中进程持有旧代租约，继续使用自己的已绑定路径；无占用
旧代会回收。迟到的旧进程不能重新发布旧版本为当前。运行时兼容指纹包含 dnr/Deno
构建输入、V8 版本与 cached-data version tag、系统及架构；不兼容编译缓存独立换代。

转译缓存校验实际源码、媒体类型、转译选项及内联源码映射策略。字节码缓存使用 Deno
提供的源码摘要、模块/脚本身份及兼容指纹，函数编译额外覆盖参数列表。一个模块身份
只保存最新源码的记录。扩展和完整解包文件读取实际磁盘内容，mtime 相同不代表未修改。

缓存文件原子替换；同内容原生文件可硬链接，转译和字节码还必须具有相同 URL、源码键
及兼容指纹。跨文件系统或没有可用索引时正常复制、解压或编译。普通 VFS 读取不会看到
转译后的 JS，也不会将缓存作为可写的包内文件。

缓存覆盖 ESM、CJS、晚加载文件模块、Web/Node Workers 和可接入的 Node/Deno 内部脚本。
无稳定身份的 data/blob 源不持久化；V8 内部 eval/new Function、WebView 页面和 WASM
不在持久缓存保证内。写缓存失败或缓存损坏会回退到正常编译；原生文件无法准备则仍报错。

SQLite 只用于管理和冷缓存去重查询。暖缓存不查数据库；文件发布后由后台线程批量更新。
最后使用时间独立于索引更新时间：应用执行开始、运行中的缓存访问及退出时异步记录，
同路径运行中更新最多每分钟一次；管理命令不刷新使用时间。正常退出补齐队列，异常终止可能漏记。
索引可能滞后或在异常退出时漏更新，数据库不可用不影响执行。大小是逻辑文件大小，
不能直接视为硬链接去重后的物理磁盘占用。同路径替换也不构成整个缓存中心的容量上限。

## 安装

```sh
dnr install app.dnp                         # 只预热当前平台原生组
dnr install app.dnp ./installed             # 默认 native：dnp + 包旁原生组
dnr install app.dnp ./installed --force      # 更新 native 安装
dnr install app.dnp ./source --mode full     # 逻辑完整解包，不保留 dnp
dnr ./source                               # 保留 appId 和 v4 缓存上下文
dnc install app.dnp ./staging --target linux_x64_glibc
```

指定目录安装只写目标安装内容，不预编译代码、不访问用户缓存中心。运行时先选择完整
有效的包旁原生组，再回退用户原生缓存；不会混合两个来源，也不会自动修改包旁副本。
v0.5.0 的包旁布局为 `<package>.unpacked/<group>/root/<logical-path>`；
同组的 `receipt.json` 保存格式、内容身份和平台。一个包旁目录只保存一个版本、一个目标平台。
安装先准备完整暂存目录，取得安装与组租约后替换；运行中拒绝覆盖。
读取仍兼容旧的 `v4/<contentHash>/<target>/<group>/root` 布局，不自动改写已有安装。
组内原有目录、资源相对位置和符号链接保持不变；用户缓存的分代布局保持不变。
V8/转译缓存始终在用户缓存中心生成，安装目录只读也不改变这一规则。

`full` 恢复所有公共文件和选定平台文件的逻辑路径，保留资源、权限和符号链接，写入
`.dnr/install.bin` 及安装租约。`--force` 只允许替换已有完整安装；运行中完整安装拒绝
替换，避免懒加载读到另一版本。此模式的源码是可修改的磁盘文件。直接用 Node/Deno
执行入口仍取决于应用 API 兼容性；只有 `dnr <目录>` 使用安装身份和缓存上下文。

`dnc install` 复用同一实现但必须指定目录；不初始化 V8/GUI，适合发行包构建。
`dnr extract` 仍导出物理 ZIP，不能与 full 逻辑安装混用。

## 管理与诊断

```sh
dnr cache info                             # 总路径、逻辑大小和索引时间
dnr cache list                             # 按来源路径展开明细
dnr cache info --json                      # 整数 bytes 和完整标识
dnr cache info --path /absolute/path/app.dnp
dnr cache clean --path '/absolute/path/*.dnp' --dry-run
dnr cache clean --stale --dry-run
dnr cache clean --trace --dry-run
dnr cache clean --all
dnr clean --before 2026-09-01               # 本地日期零点，也接受带时区 RFC 3339
dnr clean --max-size 2GB                    # 按最后使用时间清理到小于目标
dnr clean --path '/private/**/*.dnp' --dry-run
dnr cache rebuild
dnr cache info --directory ./installed
dnr --no-code-cache app.dnp
dnr --no-code-cache --no-transpile-cache app.dnp
DNR_CACHE_STATS=1 dnr app.dnp
DNR_PROFILE=1 dnr app.dnp                    # 本地转译与缓存写入耗时
```

`dnr clean` 是 `dnr cache clean` 的同义入口；直接运行名为 clean 的脚本请用 `dnr ./clean`。
大小自动显示为 B/KB/MB/GB（十进制），标注逻辑大小；硬链接重复计入，不代表真实磁盘占用。
无参数 info 汇总缓存路径、逻辑总大小、路径/缓存代数和索引更新时间；list 展开明细。
指定路径时核对磁盘，显示应用身份、缓存位置、最后使用时间及租约状态。

日期选择严格早于指定时刻的路径；未知使用时间不参与日期清理。
容量清理按使用时间从旧到新，未知时间优先、同时间按路径排序。容量目标针对整个用户缓存，
其他筛选条件取交集并限制可删除对象；无法达到目标会明确报告。
`--path` 匹配规范化的完整绝对来源路径。没有通配符时精确匹配；`*`、`?` 和 `[]` 匹配单层路径片段，`**` 可以跨目录。请给通配符参数加引号，避免由 shell 提前展开。相对模式从当前工作目录解析。原有 `--path-regex` 仍可用于高级正则筛选。
`--trace` 遍历缓存收据记载的来源路径：来源 `.dnp` 或完整安装描述已不存在时清理该路径的缓存；来源仍存在时比较其 v4 内容身份，只清理不匹配的旧代。无法读取或验证的来源会显示 `skipped-unreadable` 并保留缓存以供人工处理。`--trace` 支持 `--dry-run`、`--json` 和其他筛选条件。
智能选项不接受 `--directory`，不清理包旁安装或应用配置。

清理跳过存在任何活动缓存代的整个来源路径；`--stale` 选择已删除来源或非当前代，但不会读取当前 `.dnp` 核对版本。`rebuild` 从文件重建
SQLite，保留可读的历史访问记录，不把重建时间当作使用时间。默认 list/info 使用近似索引统计，指定路径时检查该路径的实际状态。
旧格式缓存不迁移、不访问，也不由新版缓存命令管理；确认旧版进程已退出后可另行删除旧 `v2/` 目录。
独立于 dnr 生命周期的外部进程不继承租约，清理前应停止这类程序。

运行测试见 `VALIDATION.md`；同内容启动对照工具为 `scripts/bench-startup-cache.py`，
独立包打开对照为 `cargo run --release -p dnr-package --example open_performance -- <v4包A> <v4包B>`。

`DNR_PROFILE` 默认关闭；打开时只计时本地 AST 转译和缓存写入调用，
日志输出不计入这些计时。上游 V8 core 编译探针已移除，以减少升级冲突；历史测量保留在验证记录中。
正式端到端基准必须关闭探针。
