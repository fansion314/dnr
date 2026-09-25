# 原生组件、分组解压和跨平台包

新 dnc 和 dnr 仅支持 format v3；v1/v2 包必须重新打包。原生组件统一按声明组准备，不再提供单库临时解压。编译缓存与两种安装模式见 [缓存与安装](CACHE.md)。

## 工作流

```sh
dnc scan prepared --output dnr.package.json
# 检查生成的配置，补全资源、平台映射和真实 Node-API 要求。
dnc prepared --entry main.ts --package-config dnr.package.json -o app.dnp
dnr app.dnp
```

`scan` 只输出建议配置和诊断。它识别常见 ELF、Mach-O、PE、原生扩展后缀、执行位及
shebang，参考 npm 的 `os` / `cpu` / `libc` 和 `bin` 元数据，并建议按 npm 模块目录分组。它不执行文件，不解析 JavaScript 调用图，
不收集动态链接依赖，不保证知道资源边界。扫描给出的平台及 Node-API 值必须由作者检查。
普通 JS/TS shebang 入口不必作为原生可执行文件声明。

准备依赖仍由作者负责。可以分别准备平台目录，或使用包管理器支持的多平台安装功能。
dnc 不下载 npm 包、不执行 install/postinstall、不编译、不修改动态链接路径。

## 配置

`files` 是最终包内路径的 glob。`from` 相对配置文件，`to` 为逻辑包内路径。
平台产物最好放在 prepared 目录之外，避免它们同时作为普通文件重复入包。

```json
{
  "schemaVersion": 1,
  "targets": {
    "darwin_arm64": { "os": "darwin", "arch": "arm64" },
    "linux_x64_glibc": { "os": "linux", "arch": "x64", "libc": "glibc" }
  },
  "groups": [{
    "id": "media",
    "files": ["node_modules/media/**"],
    "variants": {
      "darwin_arm64": [{ "from": "native/media_darwin_arm64", "to": "node_modules/media/native" }],
      "linux_x64_glibc": [{ "from": "native/media_linux_x64_glibc", "to": "node_modules/media/native" }]
    },
    "native": {
      "executables": ["node_modules/media/native/bin/convert"],
      "libraries": ["node_modules/media/native/lib/**"],
      "addons": [{ "path": "node_modules/media/native/addon.node", "napi": 8 }]
    }
  }]
}
```

- `native.executables` 和 `native.libraries` 支持 glob；`addons` 是具体文件及其最低
  Node-API 版本。Node/V8 ABI 插件不能仅通过改写这个数字变成 Node-API 插件。
- group 可以包含 JS 包装代码、动态库、程序、资源、目录和包内符号链接。
- 所有要通过 dnr 加载或执行的包内原生入口都必须声明。系统命令和包外磁盘库维持原行为。
- 同一平台下不允许逻辑文件冲突或多个 group 争用同一文件。
- group 中符号链接必须指向同组文件或完全属于该组的目录；跨组 OS 相对依赖需要合并组。
- glob 在构建时展开，运行时只使用已生成的具体索引。
- 公共成员适用于所有声明平台。已经识别出平台或架构的公共二进制必须与每个平台一致。
  不同平台二进制应放进 `variants`，不能把本机产物当成跨平台公共文件。
- Linux 明确区分 `glibc` 和 `musl`。当前 native runtime 验收范围仍为 macOS ARM64
  与 Linux x86_64；配置可描述其他平台，但不因此提供对应运行时。
- ABI、最低系统版本、CPU 指令要求和系统依赖仍需要作者验证。

v3 将变体保存在短物理路径 `.dnr/p/<编号>-<文件名>`，运行时按元数据还原逻辑路径。
第三方原有 `prebuilds/<platform>-<arch>` 等布局也可以保留。

桌面打包可同时传 `--desktop-manifest` 与 `--package-config`。前者描述图标、启动器和
平台安装元信息，后者描述应用内容；两者是不同配置。

## 何时落盘、如何定位

仅在 Node-API/FFI 原生加载或 Deno/Node 原生程序执行时，准备整个 group：

1. 检查 `<package>.unpacked` 中内容身份和平台匹配、校验完整的 group。
2. 否则检查用户持久缓存。
3. 不存在可复用副本时，解压公共成员及当前平台变体，校验后整体发布。

不能只凭一个文件存在就复用整个组，也不从多个位置拼接一个组。
普通 JS/TS 模块读取、文件读取和目录枚举继续使用 ZIP，不触发落盘。
`install` 可以显式提前准备当前平台所有组。

组内 JS 模块使用与将来落盘位置一致的模块路径，VFS 同时识别这些路径。
因此 `__dirname`、`import.meta.url` 生成的同组资源路径可以在原生调用完成准备后交给
外部程序。路径可以在文件实际落盘前确定；同一进程中已发出的组路径不会切换位置。

打包者应把生成这些路径的包装代码及其资源归入同组。dnr 不解析任意 shell 命令文本，
不重写任意字符串参数，也不会仅因为启动系统命令读取一个资源就推测应该解压哪一组。
没有包内原生入口触发的外部资源用法，应提前 `install` 或调整调用组织。

映射路径中不存在于 ZIP 的文件，其普通文件系统读写回退到包所在目录，不写入原生缓存。

调用者 cwd 保留。原生执行会先准备组再启动子进程，因此可将已准备的组目录传为 cwd。
普通 `chdir` 仍要求真实目录，不能靠它触发解压。

ZIP 节点保持只读，缓存中的普通文件去掉写位、程序保留执行位。用户数据不能写进组。
这不是安全沙箱：应用及其原生代码仍具有宿主用户的权限。

## 安装和缓存管理

```sh
dnr install app.dnp                  # 只预热用户缓存
dnr install app.dnp ./installed      # 复制 dnp 并准备旁置组
dnr install app.dnp ./installed --force  # 更新已存在的不同内容
dnr cache list
dnr cache info
dnr cache clean --package <id-prefix> --dry-run
dnr cache clean --package <id-prefix>
dnr cache clean --all
dnr cache info --directory ./installed
dnr cache clean --all --directory ./installed
```

普通缓存位于 macOS `~/Library/Caches/dnr` 或 Linux `${XDG_CACHE_HOME:-$HOME/.cache}/dnr`。
`DNR_CACHE_DIR` 可指定绝对路径，不可执行或不可写的目录会使相应操作失败，不回退到 tmp。

用户原生缓存为 `v3/<pathHash>/generations/<contentHash>/native/<target>/<group>/root/...`；
旁置为 `<package>.unpacked/v3/<contentHash>/<target>/<group>/root/...`。
contentHash 是规范化二进制元数据的 SHA-256，覆盖文件摘要、路径与分组语义；
不包含 shell、ZIP 压缩布局和时间戳，也不是发行者签名。完整布局见 [CACHE.md](CACHE.md)。

首次解压先写暂存目录，验证大小、CRC、SHA-256 和链接后原子发布；进程间锁协调准备，
运行实例持有共享租约，退出时释放租约并保留文件。每个 Package 实例对一个组的首次复用会完整校验，并持有租约；同实例后续命中不重复探测或校验。首次校验仍有磁盘读取成本，
但不重复解压和写入 payload。损坏的源 ZIP 不会触发同名磁盘回退。

清理默认只操作用户缓存；显式 `--directory` 才处理指定安装目录的旁置内容。
正在被 dnr 实例租用的组会跳过；短 ID 有歧义时拒绝操作。清理不会删除 `.dnp` 或应用数据。
独立于 dnr 生命周期的外部进程不在租约管理范围内，清理前应停止这类程序。
首版不自动按容量或时间淘汰。同一 Package 实例只探测一次旁置版本目录是否存在，运行期间新建的旁置安装在下一次启动时重新发现；已发出的路径保持稳定。

## 兼容性验证

```sh
cargo test --workspace
DNR_BIN="$PWD/dist/dnr" cargo test -p dnr-package --test runtime --test runtime_native --test runtime_groups --test runtime_cache -- --ignored
```

`runtime_groups` 使用真实 Node-API 插件、相邻依赖动态库、原生程序及资源路径，覆盖
Deno/Node 同步和异步子进程、懒读取、重复启动、并发冷启动和旁置安装。原生测试默认忽略，
普通工作区测试通过不等于 runtime 通过。实际执行记录见 `VALIDATION.md`。
