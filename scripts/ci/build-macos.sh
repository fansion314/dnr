#!/usr/bin/env bash
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"
[[ $(uname -sm) == 'Darwin arm64' ]] || exit 1
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
if [[ ${GITHUB_REF:-} == refs/tags/* ]]; then
    [[ ${GITHUB_REF_NAME:?} == "v$version" ]] || exit 1
fi
[[ $(git rev-parse HEAD) == "${GITHUB_SHA:?}" ]] || exit 1
export MACOSX_DEPLOYMENT_TARGET=15.0
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}

# Independent, pinned source trees; never use or modify a developer's mirrors.
upstreams=$(mktemp -d "${RUNNER_TEMP:?}/dnr-upstreams.XXXXXX")
trap 'rm -rf "$upstreams"' EXIT
for upstream in deno laufey; do
    case $upstream in
        deno) url=https://github.com/denoland/deno.git; revision=abd22074e47c6a5cd14e9e4e84743f084aa5a575 ;;
        laufey) url=https://github.com/littledivy/laufey.git; revision=1fe87874288e8359fa3de04d18cc14f56957b000 ;;
    esac
    git init "$upstreams/$upstream"
    git -C "$upstreams/$upstream" remote add origin "$url"
    git -C "$upstreams/$upstream" fetch --depth=1 origin "$revision"
    git -C "$upstreams/$upstream" checkout --detach FETCH_HEAD
    [[ $(git -C "$upstreams/$upstream" rev-parse HEAD) == "$revision" ]] || exit 1
done
cargo run --locked -p xtask -- prepare --deno "$upstreams/deno" --laufey "$upstreams/laufey"
cargo run --locked -p xtask -- build --backend webview
c++ -std=c++17 -pthread integration/native/tests/document_security.cc -o dist/document-security-test
dist/document-security-test
cargo test --locked --workspace
DNR_BIN="$root/dist/dnr" cargo test --locked -p dnr-package \
    --test runtime --test runtime_native --test runtime_groups --test runtime_cache \
    --test runtime_backend --test runtime_node_flags --test runtime_zoom --test runtime_state --test runtime_sync -- --ignored
DNR_BIN="$root/dist/dnr" DNC_BIN="$root/dist/dnc" \
    DNC_TEST_ICON=/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/GenericApplicationIcon.icns \
    cargo test --locked -p dnc --test cli macos_ -- --ignored
bash scripts/ci/package-macos.sh "$version"
{
    printf 'commit=%s\nbackend=webview\nMACOSX_DEPLOYMENT_TARGET=%s\n' "$GITHUB_SHA" "$MACOSX_DEPLOYMENT_TARGET"
    sw_vers
    uname -sm
    rustc --version
    cargo --version
    xcodebuild -version
    cmake --version
    otool -L dist/dnr dist/dnc
} > dist/macos-release/macos-build-environment.txt
