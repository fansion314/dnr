#!/usr/bin/env bash
set -euo pipefail

tag=${RELEASE_TAG:-${GITHUB_REF_NAME:?missing tag}}
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1
version=${tag#v}
repository=${GITHUB_REPOSITORY:?}
archive="dnr-$version-macos-arm64.tar.gz"
revision=${DNR_PACKAGE_REVISION:-0}
[[ $revision =~ ^(0|[1-9][0-9]*)$ ]] || exit 1
formula_asset=dnr.rb
if [[ $revision != 0 ]]; then
    archive="dnr-$version-macos-arm64-r$revision.tar.gz"
    formula_asset="dnr-macos-r$revision.rb"
fi
notes="docs/releases/$tag.md"
[[ -f $notes ]] || exit 1
# The checkout must still correspond to the public tag, including annotated tags.
remote_commit=$(git ls-remote origin "refs/tags/$tag^{}" | cut -f1)
if [[ -z $remote_commit ]]; then
    remote_commit=$(git ls-remote origin "refs/tags/$tag" | cut -f1)
fi
if [[ $revision != 0 ]]; then
    # A repair keeps the published tag/assets intact and records its new source
    # commit in BUILD-INFO.json. It must descend from that release's source.
    [[ ${GITHUB_EVENT_NAME:-} == workflow_dispatch ]] || exit 1
    [[ $remote_commit == "$(git rev-parse "refs/tags/$tag^{commit}")" ]] || exit 1
    git merge-base --is-ancestor "$remote_commit" "${GITHUB_SHA:?}"
    [[ $(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])') == "$version" ]] || exit 1
    gh release view "$tag" --repo "$repository" >/dev/null
elif [[ $remote_commit != "${GITHUB_SHA:?}" ]]; then
    echo 'Release tag no longer matches this build commit' >&2
    exit 1
fi
(cd release-assets && shasum -a 256 --check "$archive.sha256")
python3 scripts/ci/homebrew.py render "$repository" "$version" \
    "release-assets/$archive" release-assets/expected-dnr.rb
cmp release-assets/dnr.rb release-assets/expected-dnr.rb
rm release-assets/expected-dnr.rb
if [[ $formula_asset != dnr.rb ]]; then cp release-assets/dnr.rb "release-assets/$formula_asset"; fi

# Arch and macOS publication jobs share a concurrency group. Do not replace the
# Arch notes, checksums or latest-release selection when adding macOS assets.
if ! gh release view "$tag" --repo "$repository" >/dev/null 2>&1; then
    gh release create "$tag" --repo "$repository" --verify-tag --draft \
        --title "dnr $tag" --notes-file "$notes" --latest=false
fi
existing=$(mktemp -d)
trap 'rm -rf "$existing"' EXIT
gh release view "$tag" --repo "$repository" --json assets --jq '.assets[].name' > "$existing/names"
for asset in "$archive" "$archive.sha256" "$formula_asset"; do
    if grep -Fxq "$asset" "$existing/names"; then
        gh release download "$tag" --repo "$repository" --pattern "$asset" --dir "$existing"
        # Never silently replace bytes already referenced by a checksum-pinned tap.
        cmp "release-assets/$asset" "$existing/$asset"
    else
        gh release upload "$tag" "release-assets/$asset" --repo "$repository"
    fi
done
gh release edit "$tag" --repo "$repository" --draft=false
if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
    printf 'macOS ARM64 package: [%s](https://github.com/%s/releases/tag/%s)\n' \
        "$archive" "$repository" "$tag" >> "$GITHUB_STEP_SUMMARY"
fi
