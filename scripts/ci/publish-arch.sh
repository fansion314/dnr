#!/usr/bin/env bash
set -euo pipefail

tag=${GITHUB_REF_NAME:?missing tag}
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
version=${tag#v}
notes="docs/releases/$tag.md"
[[ -f $notes ]]

# A rerun of an older commit must not publish over a force-moved tag.
remote_commit=$(git ls-remote origin "refs/tags/$tag^{}" | cut -f1)
if [[ -z $remote_commit ]]; then
    remote_commit=$(git ls-remote origin "refs/tags/$tag" | cut -f1)
fi
[[ $remote_commit == "$GITHUB_SHA" ]]

for variant in dnr dnr-webview; do
    source "packaging/aur/$variant/PKGBUILD"
    [[ $pkgver == "$version" ]]
    package_file="$variant-$pkgver-$pkgrel-x86_64.pkg.tar.zst"
    [[ -f release-assets/$package_file ]]
    (cd release-assets && sha256sum --check "$package_file.sha256")
done
(
    cd release-assets
    sha256sum ./*.pkg.tar.zst > SHA256SUMS
)

if gh release view "$tag" --repo "$GITHUB_REPOSITORY" >/dev/null 2>&1; then
    gh release edit "$tag" --repo "$GITHUB_REPOSITORY" \
        --title "dnr $tag" --notes-file "$notes" --latest
else
    gh release create "$tag" --repo "$GITHUB_REPOSITORY" --verify-tag --draft \
        --title "dnr $tag" --notes-file "$notes"
fi
gh release upload "$tag" release-assets/* --repo "$GITHUB_REPOSITORY" --clobber
gh release edit "$tag" --repo "$GITHUB_REPOSITORY" --draft=false --latest
