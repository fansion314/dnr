# macOS ARM64 预构建包与 Homebrew

macOS 只提供一个 `dnr-<version>-macos-arm64.tar.gz`：`bin/dnr` 为系统 WebView
后端，`bin/dnc` 为打包器。两者版本相同，不分拆成运行时包和开发包。
压缩包还包括文档和许可证；SHA-256 文件与 `dnr.rb` 配方是附属元数据。

预构建目标为 Apple Silicon、macOS 15 Sequoia 及以上。两个程序只允许依赖
`/usr/lib` 和 `/System/Library` 中的系统库，不要求用户安装 Rust、Deno、CMake 或 CEF。
使用 ad-hoc 签名，不包含 Developer ID 签名或公证。较低 macOS 版本的源码构建
能力与这个预构建包的支持范围不同，不能据此宣称已通过旧系统验收。

## 用户安装

以下命令在维护者完成首次 Release 与 tap 配置后可用：

```sh
brew tap fansion314/dnr
brew install fansion314/dnr/dnr
dnr --version
dnc --version

# 后续更新
brew update
brew upgrade fansion314/dnr/dnr
```

也可以直接执行 `brew install fansion314/dnr/dnr`，由 Homebrew 自动添加 tap。
配方下载固定版本的 GitHub Release 压缩包并验证 SHA-256，然后一次安装两个程序；
不在用户电脑上编译。若已有手动安装的同名程序，使用 `which -a dnr dnc` 检查 PATH。

## 首次配置（维护者）

1. 创建公开仓库 `fansion314/homebrew-dnr`，以 `main` 为默认分支，并用 README 初始化。
   配方由 CI 写入 `Formula/dnr.rb`；不需要该仓库再执行一次 bottle 构建。
2. 创建专用 SSH key pair，将公钥加入 tap 仓库的 Deploy keys 并勾选写权限。
   在 `fansion314/dnr` 的 Actions secrets 中将私钥保存为 `HOMEBREW_TAP_DEPLOY_KEY`。
   该 key 只用于 tap 仓库，不使用开发者个人 SSH key 或 GitHub 登录令牌。
3. 在 `fansion314/dnr` 的 Actions variables 中设置
   `HOMEBREW_TAP=fansion314/homebrew-dnr`。更换 tap 时修改此变量及用户安装文档。
4. 将工作流与脚本提交到默认分支。先手动运行 **Release macOS package** 验证构建；
   普通分支试跑只上传 Actions artifacts，不写 Release 或 tap。
5. 按项目发布约定，在一次发布提交中准备版本号、锁文件、现有 Arch 配方及
   `docs/releases/v<version>.md`，然后推送对应 `vX.Y.Z` 标签。
   不要移动已有标签来补入此工作流；已发布的 `v0.4.1` 没有此工作流。

未设置 `HOMEBREW_TAP` 时，Release 正常上传 macOS 包，job summary 提示尚未启用 tap。
设置变量后，缺少或失效的 deploy key 会让 tap job 明确失败，已上传的 Release 包仍保留。
不要将私钥写入配方、脚本、普通变量或日志。手动使用 Contents API 时，脚本的 `update`
子命令也支持通过 `GH_TOKEN` 提供仅限 tap 仓库、具有 Contents 写权限的 fine-grained PAT。

## CI 与失败恢复

工作流为 [release-macos.yml](../.github/workflows/release-macos.yml)：

1. `macos-15` 原生 ARM64 runner，稳定 Rust，按锁文件安装依赖；使用独立的固定
   Deno/Laufey 检出，不复用开发机 `.upstream` 或 `target`。保持已有 sccache 配置。
2. 显式构建 `--backend webview`，运行 workspace 与 runtime/native/group/cache/backend/
   Node flags 测试。检查架构、版本、系统动态库和签名后，生成一个 gzip tar 包。
3. 临时 Homebrew tap 从本地压缩包安装；`brew test` 验证两份版本、TS 执行、资源读取
   和 `dnc` → `.dnp` → `dnr`。测试后卸载临时 formula 和 tap，不覆盖已有 Homebrew dnr。
4. 标签发布任务校验标签提交、包校验和与配方。macOS 与 Arch 共享发布互斥锁，
   macOS 不改写 Arch 的 `SHA256SUMS`、发布说明或 latest 标记。
5. 启用 tap 后，另一台 ARM64 runner 从公开 Release URL 再次安装并执行 `brew test`，
   成功后使用专用 deploy key 更新独立 tap，源码仓库不产生发布后验收提交。

CLI/包测试不能代替真实 WebView 窗口、绑定与关闭生命周期验收；CI 不将未运行的 GUI
检查写成通过。构建日志和环境记录保存在 Actions artifacts，发布后的安装证据留在
job 日志中，不追加源码仓库验收提交。

同版本已有 Release 资产只接受完全相同的内容；不能重建后用 `--clobber` 偷换 tap
引用的包。如果只有发布/tap job 失败，使用 **Re-run failed jobs** 复用已测试 artifact。
需要改变二进制时发布新版本。tap 更新会跳过旧版本和完全相同的配方，拒绝同版本内容
替换；并发写入通过普通 Git push 检测冲突，失败后可重跑该 job，不强推。

## 本地验证脚本

```sh
python3 -m unittest discover -s scripts/ci/tests -v
# 使用已构建的 dist/dnr、dist/dnc；不会重新编译，也不是发布操作。
bash scripts/ci/package-macos.sh 0.4.2 dist/macos-local-validation
bash scripts/ci/test-homebrew.sh dist/macos-local-validation/dnr.rb \
  dist/macos-local-validation/dnr-0.4.2-macos-arm64.tar.gz
```

本地生成的配方仅用于验证，正式配方以 Actions 发布的实际包校验和为准。

依据：[Homebrew tap 维护指南](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)、
[Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)、
[GitHub 托管 runner](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job)。
