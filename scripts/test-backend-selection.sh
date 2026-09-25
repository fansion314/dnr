#!/usr/bin/env bash
# Linux dispatch regression tests; requires CEF headers, no GUI or libcef load.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
"${CXX:-c++}" -std=c++20 -Wall -Wextra -Werror \
    -DDNR_SYSTEM_CEF=1 -DDNR_WEBVIEW=1 -DDNR_LAZY_GUI=1 -DCEF_API_VERSION=14900 \
    -I/usr/include/cef "$root/integration/native/bridge.cc" \
    "$root/integration/native/tests/selection.cc" -o "$test_dir/selection"
for scenario in headless success abi-fallback resource-fallback init-fallback early-exit explicit-cef explicit-webview after-ready helper load-fallback explicit-cef-missing explicit-webview-missing both-missing helper-missing; do
    "$test_dir/selection" "$scenario"
done
printf 'Backend dispatch: 15 scenarios passed\n'
