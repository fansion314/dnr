#!/usr/bin/env bash
set -euo pipefail

# Package already-built binaries. CI calls this only after its runtime tests pass.
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"
version=${1:?usage: package-macos.sh version [output-directory]}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1
[[ $(uname -sm) == 'Darwin arm64' ]] || exit 1
output=${2:-"$root/dist/macos-release"}
mkdir -p "$output"
output=$(cd "$output" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
name="dnr-$version-macos-arm64"
package="$stage/$name"
mkdir -p "$package/bin" "$package/licenses" "$package/docs"
for binary in dnr dnc; do
    [[ $(lipo -archs "dist/$binary") == arm64 ]] || exit 1
    actual=$("dist/$binary" --version)
    printf '%s\n' "$actual"
    if [[ $(printf '%s\n' "$actual" | awk '{print $1 " " $2}') != "$binary $version" ]]; then
        echo "Expected $binary $version; refusing to package a mismatched binary" >&2
        exit 1
    fi
    # Reject accidental links to the build machine's Homebrew or build directories.
    otool -L "dist/$binary" | tail -n +2 | awk '{print $1}' > "$stage/libraries"
    if grep -Ev '^(/usr/lib/|/System/Library/)' "$stage/libraries"; then
        echo "Unexpected non-system dependency in $binary" >&2
        exit 1
    fi
    install -m 755 "dist/$binary" "$package/bin/$binary"
    codesign --verify --strict "$package/bin/$binary"
done
dist/dnr --version | grep 'webview'
install -m 644 LICENSE "$package/licenses/LICENSE-DNR"
install -m 644 .upstream/deno/LICENSE.md "$package/licenses/LICENSE-DENO"
install -m 644 .upstream/laufey/LICENSE "$package/licenses/LICENSE-LAUFEY"
install -m 644 README.md README.zh.md THIRD_PARTY.md "$package/"
cp docs/*.md "$package/docs/"
COPYFILE_DISABLE=1 tar -czf "$output/$name.tar.gz" -C "$stage" "$name"
(
    cd "$output"
    shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256"
)
python3 scripts/ci/homebrew.py render "${GITHUB_REPOSITORY:-fansion314/dnr}" \
    "$version" "$output/$name.tar.gz" "$output/dnr.rb"
echo "Packaged $output/$name.tar.gz"
