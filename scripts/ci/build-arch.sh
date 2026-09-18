#!/usr/bin/env bash
set -euo pipefail

variant=${1:?usage: build-arch.sh dnr|dnr-webview tag commit}
tag=${2:?missing release tag}
commit=${3:?missing commit}
[[ $variant == dnr || $variant == dnr-webview ]]
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
[[ $commit =~ ^[0-9a-f]{40}$ ]]
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
recipe="$root/packaging/aur/$variant/PKGBUILD"
build_root="$root/dist/ci-$variant"
output="$root/dist/release"

if (( EUID == 0 )); then
    # This script runs only inside the disposable official Arch container.
    source "$recipe"
    pacman -Syu --noconfirm --needed base-devel sudo git ca-certificates \
        "${depends[@]}" "${makedepends[@]}"
    useradd --create-home --uid "${DNR_BUILD_UID:?missing build uid}" builder
    mkdir -p "$build_root/src" "$output" /cache/cargo
    chown -R builder:builder "$build_root" "$output" /cache
    exec runuser -u builder -- env \
        CARGO_HOME=/cache/cargo CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_DEBUG=0 \
        bash "$0" "$variant" "$tag" "$commit"
fi

cd "$root"
[[ $(git rev-parse HEAD) == "$commit" ]]
source "$recipe"
[[ v$pkgver == "$tag" ]]
[[ $(python -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])') == "$pkgver" ]]
export SOURCE_DATE_EPOCH
SOURCE_DATE_EPOCH=$(git show -s --format=%ct "$commit")

# Shallow, pinned upstream checkouts avoid downloading their entire Git history.
# The application snapshot is exactly the commit that triggered this tag run.
git archive "$commit" | tar -x -C "$build_root/src" --one-top-level=dnr
for upstream in deno laufey; do
    case $upstream in
        deno) url=https://github.com/denoland/deno.git; revision=$_deno_commit ;;
        laufey) url=https://github.com/littledivy/laufey.git; revision=$_laufey_commit ;;
    esac
    git init "$build_root/src/$upstream"
    git -C "$build_root/src/$upstream" remote add origin "$url"
    git -C "$build_root/src/$upstream" fetch --depth=1 origin "$revision"
    git -C "$build_root/src/$upstream" checkout --detach FETCH_HEAD
    [[ $(git -C "$build_root/src/$upstream" rev-parse HEAD) == "$revision" ]]
done

cp "$recipe" "$build_root/PKGBUILD"
cat > "$build_root/makepkg.conf" <<'CONFIG'
source /etc/makepkg.conf
# Release packages must run on generic x86_64, not just the runner's CPU.
CFLAGS='-march=x86-64 -mtune=generic -O2 -pipe'
CXXFLAGS="$CFLAGS"
RUSTFLAGS='-C target-cpu=x86-64'
CONFIG

(
    cd "$build_root"
    source ./PKGBUILD
    srcdir="$build_root/src"
    prepare
)
cd "$build_root"
# Sources were prepared above; dependency checks, build, check and package still run.
makepkg --config "$build_root/makepkg.conf" --noextract --force --noconfirm
package_file="$variant-$pkgver-$pkgrel-x86_64.pkg.tar.zst"
[[ -f $package_file ]]

mkdir -p installed
bsdtar -xf "$package_file" -C installed usr/bin
installed/usr/bin/dnr --version | tee "$output/$variant-version.txt"
installed/usr/bin/dnc --version | tee -a "$output/$variant-version.txt"
ldd installed/usr/bin/dnr | tee "$output/$variant-libraries.txt"
if grep -q 'not found' "$output/$variant-libraries.txt"; then
    exit 1
fi
if [[ $variant == dnr ]]; then
    installed/usr/bin/dnr --check-system-cef
fi
cp "$package_file" "$output/"
(
    cd "$output"
    sha256sum "$package_file" > "$package_file.sha256"
)
{
    printf 'tag=%s\ncommit=%s\nvariant=%s\n' "$tag" "$commit" "$variant"
    cat /etc/os-release
    rustc --version
    cc --version
    pacman -Q
} > "$output/$variant-build-environment.txt"
