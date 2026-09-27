#!/usr/bin/env bash
set -euo pipefail

# Optional archive argument substitutes only the URL, keeping the real checksum.
formula=${1:?usage: test-homebrew.sh formula [local-archive]}
archive=${2:-}
export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_CLEANUP=1 HOMEBREW_NO_ANALYTICS=1
export HOMEBREW_DEVELOPER=1
tap="dnr-ci/validation-$$"
if brew list --versions dnr >/dev/null 2>&1; then
    echo 'Refusing to replace an existing Homebrew dnr installation.' >&2
    exit 1
fi
brew tap-new --no-git "$tap"
cleanup() {
    # This tap is unique to this run; never unlink or uninstall another tap's dnr.
    brew uninstall "$tap/dnr" >/dev/null 2>&1 || true
    brew untap "$tap" >/dev/null 2>&1 || true
}
trap cleanup EXIT
tap_path=$(brew --repository "$tap")
cp "$formula" "$tap_path/Formula/dnr.rb"
if [[ -n $archive ]]; then
    python3 - "$tap_path/Formula/dnr.rb" "$archive" <<'PY'
from pathlib import Path
import re
import sys
formula, archive = map(Path, sys.argv[1:])
formula.write_text(re.sub(r'^  url ".*"$', '  url "' + archive.resolve().as_uri() + '"',
                          formula.read_text(), flags=re.M))
PY
fi
brew install --formula --skip-link "$tap/dnr"
brew test --force "$tap/dnr"
# Installation must preserve signatures and must not rewrite system-only load paths.
prefix=$(brew --prefix "$tap/dnr")
codesign --verify --strict "$prefix/bin/dnr"
codesign --verify --strict "$prefix/bin/dnc"
