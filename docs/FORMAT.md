# DNR application formats v1 / v2 / v3

dnc 0.3.0 默认输出 v3；`--format-version 2` 可输出旧格式。dnr 同时读取 v1/v2/v3。下文先描述共同封装和 v1 行为，随后描述 v2/v3 扩展。

`.dnp` 是可执行的 POSIX shell 启动头与 ZIP 区域拼接而成的文件。扩展名不参与运行时识别。

```text
#!/bin/sh
exec dnr --package "$0" --entry 'src/main.ts' -- "$@"
exit 127
# DNRZIP1
<ZIP local headers and individually compressed entries>
<ZIP central directory and end records>
```

- 头部最多 16 KiB；第一行必须是 `#!/bin/sh`，使用 LF 换行。`# DNRZIP1` 之后的第一个字节是 ZIP 区域起点。
- ZIP 内部偏移以 ZIP 区域起点为零；写入和读取都使用可 seek 的区域适配器。
- 普通文件与 manifest 使用 ZIP Zstandard method 93，压缩等级 6；目录与符号链接使用 ZIP 的对应条目。ZIP 库负责 ZIP64。
- `.dnr/manifest.json` 是保留元数据，不作为应用文件暴露；其他 `.dnr/` 条目禁止使用。

```json
{"formatVersion":1,"entry":"src/main.ts","appId":"com.example.app"}
```

入口必须是包内相对路径，并解析到包内文件。启动头中的 `--entry` 必须与 manifest 一致。dnr 从来不执行头部 shell 文本，shell 只在用户直接启动应用文件时执行它。

包内路径使用 UTF-8、`/` 分隔，禁止绝对路径、`..`、NUL、换行和反斜杠。拒绝重复路径、文件/目录冲突，以及越界或循环符号链接。include 目标路径冲突不自动覆盖。

运行时只读取中央目录、manifest 及符号链接目标来建立索引；普通文件第一次读取时才解压并校验尺寸与 CRC。缓存按文件管理，打开的文件句柄保留自己的数据引用，不因缓存淘汰而失效。数据损坏不能触发磁盘回退。

应用 ZIP 映射在应用包的真实所在目录，同名文件优先于磁盘内容，目录合并枚举。cwd 保留调用者目录。ZIP 内节点只读，其余写入作用于磁盘；没有 copy-on-write 层，运行应用不会全量解压。原生加载器是按需落盘的例外：仅将请求的原生库释放到独立系统临时目录，普通 VFS 读取仍使用 ZIP 内容。外部子进程不会继承系统级挂载。

独立的 `dnr tree <包>` 和 `dnr extract <包> <目录>` 命令直接操作包内 ZIP，不启动运行时，也不使用磁盘回退。两者包含运行时隐藏的 `.dnr/manifest.json`；解压保存其原始字节，不包含 ZIP 外的 shell 头。目标须为新目录或空的真实目录；文件经尺寸/CRC 校验后统一发布，保留空目录、符号链接和 Unix 普通权限位。

此格式不包含引擎、浏览器后端、签名、加密或依赖安装指令。不同目标平台可使用同一个纯 JS/TS 包；平台相关原生库可包含在 ZIP 中，首次加载时校验并释放到系统临时目录；同进程共享解压路径，宿主退出时清理，不在包旁留下文件。原生库的目标平台和 ABI、系统动态链接依赖仍由应用负责。

桌面 manifest 是 dnc 的构建配置，不是上述 ZIP manifest 的扩展。macOS `.app` 和
Arch/CachyOS `.pkg.tar.zst` 将未改变格式的 `.dnp` 作为应用内容封装，并在外层添加
桌面身份、图标、启动器与平台元数据；参见 [桌面打包](DESKTOP-PACKAGING.md)。


## v2：分组、平台视图和完整性索引

封装仍是同一个 shell 启动头和 ZIP 区域，`DNRZIP1` 标记表示封装方式，不表示 manifest 版本。
v2 manifest 的 `formatVersion` 为 2，增加 `targets`、`groups` 和 `integrity`：后者指向
`.dnr/index.json` 并记录其 SHA-256。运行时拒绝不认识的格式版本。

索引逐项保存逻辑路径 `path`、ZIP 路径 `source`、`kind`、`size`、`mode`、文件或链接内容
`sha256`、可选 `link`、`group`、`target`、`native` 和 `napi`。普通 ZIP 条目必须被索引覆盖，
文件来源不能重复。公共目录和平台目录可合并；同平台的逻辑文件不能覆盖。
manifest 仍限制为 64 KiB，完整文件索引单独存储并限制为 64 MiB。

允许的保留条目扩展为 `.dnr/manifest.json`、`.dnr/index.json` 与 `.dnr/payloads/` 中的
已索引变体。它们不直接出现在应用 VFS；`tree` 和 `extract` 仍按原始 ZIP 展示和导出所有平台。
运行时只建立当前平台的逻辑视图，其他平台的已声明路径不会作为同名磁盘回退入口。

普通文件读取仍懒解压并检查 CRC；v2 还检查索引中的 SHA-256。包内容身份是 manifest
原始字节的 SHA-256，通过索引哈希覆盖全部文件内容及映射规则，不需要每次启动哈希整个 ZIP。
内容身份用于版本隔离，不提供发行者认证。

v2 原生加载和执行只接受 manifest 索引中声明的对应入口。首次调用准备整个 group：
优先复用完整且匹配的 `<package>.unpacked` 副本，再检查用户缓存，否则校验并解压该组的
公共文件及当前平台变体。原生扩展不再在退出时删除。普通读取和元数据查询不触发组落盘。
组中 JS 模块的路径映射到其稳定的物理组位置，尚未落盘的内容仍由 VFS 提供。
组内相对资源与原生依赖需要形成完整集合；没有 OS 挂载或任意 shell 字符串重写。

作者配置、安装、清理命令和兼容性边界见 [原生打包](NATIVE-PACKAGING.md)。


## v3：Stored 二进制元数据与短载荷路径

封装仍用 `DNRZIP1`，普通文件仍为 Zstd level 6。第一个 ZIP 条目必须是
`.dnr/meta.bin`，Stored、不加密、不使用 data descriptor，最大 64 MiB。运行时先从
local header 定位元数据并校验 CRC，再核对中央目录名称、尺寸、方法与 CRC。
不再生成 manifest.json/index.json；两者的语义合并到二进制元数据。

所有整数为小端；字符串为 u32 字节长度＋UTF-8；可选字符串为 u8 存在标志（0/1）
及存在时的字符串。序列以 u32 项数开头。读取必须有界，拒绝截断、无效标志和尾随数据。
头部顺序为 `DNRMETA3`（8 字节）、u64 body 长度、32 字节 body SHA-256，再接 body：

1. entry、appId 两个字符串。
2. targets 序列：每项为 id、os、arch、可选 libc；按 id 排序。
3. groups 字符串序列，按名称排序。
4. 文件记录序列，按 `(path, target, source)` 排序。每项依次为 path、source、
   u8 kind（0=file、1=directory、2=symlink）、u64 size、u32 mode、u8 摘要存在标志
   及存在时的 32 字节 SHA-256、可选 link/group/target/native 四个字符串、u32 napi
   （0 表示不存在）。字段意义和分组／平台／完整性约束沿用 v2。

body 摘要即 `contentHash`，不包括自身、ZIP 时间戳、压缩方式或物理偏移。它绑定全部
记录及内容摘要；不是整包字节摘要或发行者签名。普通文件仍在实际读取时验证 CRC/SHA。
平台变体以确定性编号写入 `.dnr/p/<编号>-<原文件名>`，逻辑路径、平台和分组在元数据
中保存。含符号链接的组保留必要相对布局，移除全组与链接目标共有的逻辑前缀后放在编号容器内，避免物理解包产生越界链接。原生组实际落盘恢复原有逻辑布局；不压平外部程序依赖的相对路径。

`dnc inspect <包> --json` 输出 manifest、所有平台 records、contentHash/packageId 和
archiveEntries（含工具合成的目录名称）。`dnc cat <包> <ZIP路径>` 输出经校验的原始条目，
不应用磁盘回退。它们不启动 V8/GUI，也不要求系统 tar 支持 ZIP93。

完整安装描述 `.dnr/install.bin` 以 `DNRINST3` 开头，随后是选定 target 字符串与原始
meta.bin。它仅提供安装身份，磁盘源码可变，不能凭描述内的文件摘要跳过源码校验。
原生旁置、路径分代与编译缓存详见 [缓存与安装](CACHE.md)。
