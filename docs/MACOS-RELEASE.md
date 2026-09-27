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
brew tap fansion314/dnr https://github.com/fansion314/dnr.git
brew install fansion314/dnr/dnr
dnr --version
dnc --version

# 后续更新
brew update
brew upgrade fansion314/dnr/dnr
```

tap 与源码共用 `fansion314/dnr` 仓库，因此首次添加必须指定完整 Git URL；
省略 URL 会让 Homebrew 寻找另一个 `homebrew-dnr` 仓库。配方在根目录 `Formula/dnr.rb`。
配方下载固定版本的 GitHub Release 压缩包并验证 SHA-256，然后一次安装两个程序；
不在用户电脑上编译。若已有手动安装的同名程序，使用 `which -a dnr dnc` 检查 PATH。

## 首次配置（维护者）

1. 将工作流与脚本提交到默认分支。无需额外仓库、SSH key、PAT、secret 或变量；
   Homebrew job 使用本仓库 `GITHUB_TOKEN` 和显式 `contents: write` 权限。
   默认分支的保护规则需要允许这一配方更新写入。
2. 手动运行 **Release macOS package**，留空 `tap_release` 来验证构建；普通分支
   试跑只上传 Actions artifacts，不写 Release 或 tap。
3. 按项目发布约定，在一次发布提交中准备版本号、锁文件、现有 Arch 配方及
   `docs/releases/v<version>.md`，然后推送对应 `vX.Y.Z` 标签。
   不要移动已有标签来补入此工作流；已发布的 `v0.4.1` 没有此工作流。

发布成功后，CI 会在默认分支追加一次仅更新 `Formula/dnr.rb` 的提交，写入真实的版本、
Release URL 和 SHA-256。这是用户明确允许的配方更新步骤，不是验收记录提交；
独立 tap 也需要相同的更新提交，共仓只是将它保留在源码仓库中。

## CI 与失败恢复

工作流为 [release-macos.yml](../.github/workflows/release-macos.yml)：

1. `macos-15` 原生 ARM64 runner，稳定 Rust，按锁文件安装依赖；使用独立的固定
   Deno/Laufey 检出，不复用开发机 `.upstream` 或 `target`。保持已有 sccache 配置。
2. 显式构建 `--backend webview`，运行 workspace 与 runtime/native/group/cache/backend/
   Node flags/zoom 测试。检查架构、版本、系统动态库和签名后，生成一个 gzip tar 包。
3. 临时 Homebrew tap 从本地压缩包安装；`brew test` 验证两份版本、TS 执行、资源读取
   和 `dnc` → `.dnp` → `dnr`。测试后卸载临时 formula 和 tap，不覆盖已有 Homebrew dnr。
4. 标签发布任务校验标签提交、包校验和与配方。macOS 与 Arch 共享发布互斥锁，
   macOS 不改写 Arch 的 `SHA256SUMS`、发布说明或 latest 标记。
5. 另一台 ARM64 runner 下载公开 Release 包、核对校验和，再从公开 URL 安装并执行
   `brew test`，成功后通过 Contents API 提交本仓库 `Formula/dnr.rb`。

CLI/包测试不能代替真实 WebView 窗口、绑定与关闭生命周期验收；CI 不将未运行的 GUI
检查写成通过。构建日志和环境记录保存在 Actions artifacts，发布后的安装证据留在
job 日志中，不追加源码仓库验收提交。

同版本已有 Release 资产只接受完全相同的内容；不能重建后用 `--clobber` 偷换 tap
引用的包。如果只有发布/tap job 失败，使用 **Re-run failed jobs** 复用已测试 artifact。
需要改变二进制时发布新版本。tap 更新会跳过旧版本和完全相同的配方，拒绝同版本内容
替换；并发写入通过 Contents API 的文件 SHA 检测冲突，失败后可重跑该 job。

若只需为已经发布的包更新或恢复 tap，可在默认分支手动运行工作流并填写
`tap_release=v0.4.2`。这条路径不重建、不上传或改写 Release 资产，只下载现有包、
验证 Homebrew 安装并更新配方：

```sh
gh workflow run release-macos.yml --ref main -f tap_release=v0.4.2
```

## 本地验证脚本

```sh
python3 -m unittest discover -s scripts/ci/tests -v
# 使用已构建的 dist/dnr、dist/dnc；不会重新编译，也不是发布操作。
bash scripts/ci/package-macos.sh 0.4.2 dist/macos-local-validation
bash scripts/ci/test-homebrew.sh dist/macos-local-validation/dnr.rb \
  dist/macos-local-validation/dnr-0.4.2-macos-arm64.tar.gz
```

本地生成的配方仅用于验证，正式配方以 Actions 发布的实际包校验和为准。

## 保持版本号的 macOS 修订发布

已发布版本的 macOS 修复可以追加包修订号，不移动原标签或覆盖旧资产。例如：

```sh
gh workflow run release-macos.yml --ref main -f publish_tag=v0.4.2 -f package_revision=1
```

此路径完整重建 dnr/dnc、执行 runtime 和真实启动器测试，发布
`dnr-0.4.2-macos-arm64-r1.tar.gz`，并将配方设为 `version "0.4.2"`、`revision 1`。
用户执行 `brew upgrade fansion314/dnr/dnr` 即可获得修复；程序版本仍为 0.4.2。
修订源提交必须是原标签的后代且 workspace 版本一致；归档内的 `BUILD-INFO.json`
记录真实源提交与修订号。修订不会重建或替换原有 Linux 资产。

仅恢复这个修订的配方时使用 `-f tap_release=v0.4.2 -f package_revision=1`。
同一个版本/修订组合仍不可换包，后续修复必须增加 revision。r1 增加了默认 PATH
查找和 Finder/Homebrew 回退；旧应用需用修复后的 dnc 重新打包，显式运行时路径仍保留。

依据：[Homebrew tap 维护指南](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)、
[Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)、
[GitHub 托管 runner](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job)。
