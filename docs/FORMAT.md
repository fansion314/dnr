# DNR application format v3

新版 dnc 与 dnr 仅支持 v3。v1/v2 包在读取时明确拒绝，须用原始应用目录和新版 dnc 重新打包；不提供旧格式输出或自动迁移。历史格式见 Git 历史。

## 封装与读取规则

`.dnp` 是 POSIX shell 启动头与 ZIP 区域拼接的文件，扩展名不参与识别：

```text
#!/bin/sh
exec dnr --package "$0" --entry 'src/main.ts' -- "$@"
exit 127
# DNRZIP1
<ZIP local entries and central directory>
```

启动头最多 16 KiB，必须以 `#!/bin/sh` 开头，使用 LF。`DNRZIP1` 标识封装方式，不代表包版本；ZIP 偏移以标记之后的区域为零。ZIP 库处理 ZIP64。

入口为包内相对文件，启动头的 `--entry` 必须与元数据一致。dnr 不执行头部 shell 文本；直接执行包时才由 shell 转发。

路径为 UTF-8，使用 `/`，拒绝绝对路径、`..`、NUL、换行、反斜杠、重复路径、文件/目录冲突、越界或循环链接。所有普通 ZIP 条目必须被元数据覆盖，文件来源不能重复；每个平台分别校验逻辑视图。平台不可用路径不能回退同名磁盘文件。

普通文件按需解压，验证尺寸、CRC 和 SHA-256；元数据的内容身份不替代逐文件校验，也不是发行者签名。损坏不能触发磁盘回退。打开的句柄保留数据引用，不因进程内缓存淘汰失效。

ZIP 以包所在真实目录为根，ZIP 优先、目录合并、包内节点只读，cwd 保留调用者目录。原生加载按声明 group 整组准备，保留相邻库、资源及链接；没有 OS 挂载，外部进程不能访问内存 VFS。详见 [原生打包](NATIVE-PACKAGING.md)。

`dnr tree` 与 `dnr extract` 展示、导出实际 ZIP（含 `.dnr/meta.bin` 与所有平台），不启动应用。extract 只接受新目录或空目录，校验后发布并保留权限和链接。`install --mode full` 导出当前平台逻辑视图，两者语义不同。

格式不含运行时、签名、加密或依赖安装指令。[桌面打包](DESKTOP-PACKAGING.md) 在外层添加启动器、图标与平台身份，不改变 DNP 格式。

## 二进制元数据与短载荷路径

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
   （0 表示不存在）。native 为 addon、library 或 executable，且必须属于声明组。

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
