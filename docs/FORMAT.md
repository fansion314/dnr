# DNR application format v1

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

运行时只读取中央目录、manifest 及符号链接目标来建立索引；普通文件第一次读取时才解压并校验尺寸与 CRC。缓存按文件管理，打开的文件句柄保留自己的数据引用，不因 LRU 淘汰而失效。数据损坏不能触发磁盘回退。

应用 ZIP 映射在应用包的真实所在目录，同名文件优先于磁盘内容，目录合并枚举。cwd 保留调用者目录。ZIP 内节点只读，其余写入作用于磁盘；没有 copy-on-write 层或全量解压模式。外部子进程不会继承系统级挂载。

此格式不包含引擎、浏览器后端、签名、加密或依赖安装指令。不同目标平台可使用同一个纯 JS/TS 包；平台相关原生库需要另行部署在磁盘。
