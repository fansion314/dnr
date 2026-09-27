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
    if [[ -n ${launch_test:-} ]]; then rm -rf "$launch_test"; fi
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

# Exercise the shipped dnc's launcher against this Homebrew installation with
# a Finder-like PATH that deliberately omits both Homebrew bin directories.
launch_test=$(mktemp -d)
python3 - "$prefix" "$launch_test" <<'PY'
import json, os, pathlib, subprocess, sys
prefix, root = map(pathlib.Path, sys.argv[1:])
source = root / "input"
source.mkdir()
(source / "main.ts").write_text('console.log(JSON.stringify({runtime:Deno.execPath(),args:Deno.args}));')
manifest = root / "desktop.json"
manifest.write_text(json.dumps({"appId":"dev.dnr.homebrew.launcher", "name":"dnr launcher test",
    "version":"1.0.0", "entry":"main.ts", "macos":{"icon":
    "/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/GenericApplicationIcon.icns"}}))
app = root / "Test App.app"
subprocess.run([str(prefix/"bin/dnc"),str(source),"--desktop-manifest",str(manifest),
                "--target","macos","-o",str(app)],check=True)
result = subprocess.run([str(app/"Contents/MacOS/launcher"),"a b",""],
    cwd=root, env=dict(os.environ,PATH="/usr/bin:/bin"),check=True,capture_output=True,text=True,timeout=30)
data = json.loads(result.stdout)
assert os.path.samefile(data["runtime"],prefix/"bin/dnr"), data
assert data["args"] == ["a b", ""], data
print("DNR_HOMEBREW_LAUNCHER_OK: Finder-like PATH found the installed Homebrew runtime")
PY
