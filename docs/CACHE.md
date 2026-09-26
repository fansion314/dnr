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
包旁布局为 `<package>.unpacked/v4/<contentHash>/<target>/<group>/root/...`。
V8/转译缓存始终在用户缓存中心生成，安装目录只读也不改变这一规则。

`full` 恢复所有公共文件和选定平台文件的逻辑路径，保留资源、权限和符号链接，写入
`.dnr/install.bin` 及安装租约。`--force` 只允许替换已有完整安装；运行中完整安装拒绝
替换，避免懒加载读到另一版本。此模式的源码是可修改的磁盘文件。直接用 Node/Deno
执行入口仍取决于应用 API 兼容性；只有 `dnr <目录>` 使用安装身份和缓存上下文。

`dnc install` 复用同一实现但必须指定目录；不初始化 V8/GUI，适合发行包构建。
`dnr extract` 仍导出物理 ZIP，不能与 full 逻辑安装混用。

## 管理与诊断

```sh
dnr cache list
dnr cache info --path /absolute/path/app.dnp
dnr cache clean --path /absolute/path/app.dnp --dry-run
dnr cache clean --stale --dry-run
dnr cache clean --all
dnr cache rebuild
dnr cache info --directory ./installed
dnr --no-code-cache app.dnp
dnr --no-code-cache --no-transpile-cache app.dnp
DNR_CACHE_STATS=1 dnr app.dnp
DNR_PROFILE=1 dnr app.dnp                    # 本地转译与缓存写入耗时
```

清理跳过占用中的缓存；`--stale` 选择已删除来源或非当前代，`rebuild` 从文件重建
SQLite。默认 list/info 使用近似索引统计，指定路径时检查该路径的实际状态。
旧格式缓存不迁移、不访问，也不由新版缓存命令管理；确认旧版进程已退出后可另行删除旧 `v2/` 目录。
独立于 dnr 生命周期的外部进程不继承租约，清理前应停止这类程序。

运行测试见 `VALIDATION.md`；同内容启动对照工具为 `scripts/bench-startup-cache.py`，
独立包打开对照为 `cargo run --release -p dnr-package --example open_performance -- <v4包A> <v4包B>`。

`DNR_PROFILE` 默认关闭；打开时只计时本地 AST 转译和缓存写入调用，
日志输出不计入这些计时。上游 V8 core 编译探针已移除，以减少升级冲突；历史测量保留在验证记录中。
正式端到端基准必须关闭探针。
