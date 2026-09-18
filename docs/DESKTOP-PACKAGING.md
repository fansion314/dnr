# dnc 桌面应用打包

dnc 先将准备好的目录打成 `.dnp`，再生成 macOS ARM64 `.app` 或 Arch/CachyOS
x86_64 `.pkg.tar.zst`。不携带或安装 dnr，不构建前端、不安装依赖、不收集 import。
两种目标都在对应原生系统构建；原来的 `.dnp` 命令不变。

## 配置与命令

在应用项目中创建 `desktop.json`，例如 Songjian 的元数据：

```json
{
  "appId": "world.fansionia.songjian",
  "name": "松间",
  "version": "1.1.0",
  "description": "专注计时、待办与环境声",
  "entry": "desktop/main.js",
  "macos": {
    "icon": "desktop/icon.icns",
    "runtimePath": "/usr/local/bin/dnr",
    "minimumSystemVersion": "11.0"
  },
  "linux": {
    "packageName": "songjian",
    "icon": "desktop/icon.svg",
    "release": 1,
    "backend": "webview",
    "categories": ["Utility"],
    "keywords": ["专注", "计时", "待办", "focus", "timer"],
    "license": [],
    "depends": []
  }
}
```

`prepared/` 应已包含入口 `desktop/main.js` 及它引用的 `dist/`、依赖和其他资源。
图标路径相对于 **manifest 所在目录**，入口相对于 **输入目录**；不会把 manifest
所在项目自动当作输入。上述示例只参考 Songjian 的布局，没有修改 Songjian 的构建脚本。
`license` 应填写应用实际采用的 SPDX 许可证标识，示例不推定其许可证。

```sh
# macOS ARM64：需要 Xcode Command Line Tools，以及系统自带的签名/图标工具。
dnc prepared --desktop-manifest desktop.json --target macos -o "release/松间.app"

# Arch/CachyOS x86_64：需要 makepkg、fakeroot、zstd，以普通用户运行。
dnc prepared --desktop-manifest desktop.json --target archlinux -o release/songjian-1.1.0-1-x86_64.pkg.tar.zst
```

输出名由 `-o` 指定；Linux 实际包名/版本取自 manifest，建议文件名与其一致。
再次构建加 `--force`。新产物验证成功后才替换旧输出，打包/编译/签名失败会保留旧产物。
输出位于输入目录内时，自动排除该输出；其他历史产物仍需 `--exclude release`。

继续支持 `--include source=destination`、`--exclude archive/path`。
`--entry` 可覆盖 manifest 入口；`--app-id` 若提供必须与 manifest 相同。
未知字段会报错，避免拼写错误被忽略。`appId` 使用反向域名格式，跨平台保持一致，
同时用于 `.dnp` 存储身份、macOS Bundle ID、Linux 桌面入口及窗口身份。
`name` 支持中文、空格和引号；`version` 使用点分十进制数字，例如 `1.1.0`。
只需提供当前目标的 `macos` 或 `linux` 部分。

## macOS

输出结构：

```text
松间.app/Contents/
  Info.plist
  MacOS/launcher
  Resources/application.dnp
  Resources/AppIcon.icns
  _CodeSignature/...
```

`macos.icon` 必填，支持 `.icns` 或正方形 `.png`。PNG 通过系统 `sips` 生成
16–512 点及 2× 尺寸，再经 `iconutil` 生成 ICNS；建议使用带透明留白的 1024×1024 PNG。
不自动设计图标或给现有图标增加圆角、留白。

`runtimePath` 默认 `/usr/local/bin/dnr`，必须是目标机器上共享 dnr 的绝对路径，
不展开 `~` 或环境变量。也可以指向已安装的按哈希固定的共享运行时。dnc 不复制
该文件，也不要求构建时已安装在该位置；运行时缺失时启动器显示错误对话框。
共享 runtime 的更新和版本兼容由安装者管理。

原生 ARM64 启动器从自身 bundle 定位 `.dnp`、图标，设置 Laufey 应用名称与身份，
随后 `exec` 共享 dnr，保留参数和调用者 cwd。无需 Finder 的 PATH 包含 dnr。
`minimumSystemVersion` 默认 `11.0`，只设置 launcher/Info.plist 的最低版本；
实际最低系统版本还取决于共享 dnr 自身。

构建自动执行本地 ad-hoc 签名及 `codesign --verify --deep --strict`。
可将 `.app` 拖入 `/Applications` 或 `~/Applications`，先确保配置路径上的 dnr 已安装。
本功能不生成 DMG/PKG 安装器，不提供 Developer ID 签名、公证、深链注册或自动更新。

## Arch/CachyOS

生成临时 PKGBUILD 并调用系统 `makepkg`，由它创建 `.PKGINFO`、`.BUILDINFO`、
`.MTREE` 和 Zstd 压缩的 pacman 包。不会自动安装依赖、安装输出包或请求 root。
包默认不带 GPG 签名。

```sh
sudo pacman -U release/songjian-1.1.0-1-x86_64.pkg.tar.zst
# 更新时提高 version 或 linux.release，再重新构建并 pacman -U。
sudo pacman -R songjian
```

安装位置由包管理器追踪：

| 路径 | 内容 |
| --- | --- |
| `/usr/bin/<packageName>` | 启动脚本 |
| `/usr/lib/<appId>/application.dnp` | 应用内容 |
| `/usr/share/applications/<appId>.desktop` | 桌面菜单入口 |
| `/usr/share/icons/hicolor/scalable/apps/<appId>.svg` | SVG 图标 |
| `/usr/share/pixmaps/<appId>.png` | 使用 PNG 时的图标替代位置 |

`linux.icon` 必填，支持 SVG 或 PNG；`packageName` 必须为小写包名。
`release` 默认 `1`，必须为正整数；`categories` 默认 `["Utility"]`；
`keywords`、`license`、`depends` 默认空数组。

`backend` 默认 `webview`，自动声明 `gtk3`、`webkit2gtk-4.1` 依赖。
选择 `system-cef` 时声明 `gtk3`、`cef`，且启动前执行 `dnr --check-system-cef`，
ABI 不匹配即停止。此字段不构建或切换系统 dnr 的后端；须安装对应的共享 runtime。
`depends` 可以补充应用其他系统依赖。

为兼容现有 `/usr/local/bin/dnr` 手工安装方式，默认将 dnr 列为可选的包管理器依赖，
但 **实际运行必须有共享 dnr**。启动器先查 PATH，再尝试 `/usr/local/bin/dnr`。
若通过 pacman 管理 runtime，可设置 `"runtimePackage": "dnr"`（或实际提供它的包名），
改为强制依赖；这不会生成 runtime 包或添加软件源。

桌面入口使用 appId 作为图标名和 StartupWMClass；启动器设置相同的
`LAUFEY_APP_ID`、应用名和图标。升级/卸载不删除应用的数据目录。

Linux 的脚本并不是设置桌面图标所必需：固定路径时 `.desktop` 可直接执行 dnr，
也可以通过 `/usr/bin/env` 设置变量。这里的脚本集中处理运行时查找、环境变量和
CEF 启动前校验，同时提供终端命令；与 macOS 用于保留 bundle 身份的原生启动器不同。

## 验证

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# macOS：真实编译/签名、重建排除旧产物、启动与参数/cwd/身份。
DNR_BIN=/absolute/path/to/dnr DNC_TEST_ICON=/absolute/path/to/icon.icns cargo test -p dnc --test cli macos_bundle_real_runtime -- --ignored --nocapture
# 将 DNC_TEST_ICON 换为 PNG，覆盖图标转换。

# 原生 Arch/CachyOS，以普通用户运行：
cargo test -p dnc --test cli arch_package_metadata_and_payload -- --ignored --nocapture
```

Linux 原生测试使用 `pacman -Qip` 和 `bsdtar` 读取真实生成包，不安装到系统。
实际安装、菜单启动、窗口图标与卸载仍需原生桌面验证。普通工作区测试在 macOS
执行 Linux 文件生成、shell 转义、参数、CEF 检查失败路径和 PKGBUILD 文件复制，
不能替代完整 makepkg/pacman 验证。当前执行证据见 [VALIDATION.md](../VALIDATION.md)。

参考：[makepkg](https://man.archlinux.org/man/makepkg.8.en)、
[Arch 包格式](https://man.archlinux.org/man/alpm-package.7.en)、
[Desktop Entry 规范](https://specifications.freedesktop.org/desktop-entry/latest-single/)。
