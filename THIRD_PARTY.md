# Upstream components

dnr builds modified upstream sources locally. Original copyright and license
notices remain in the prepared source trees; those sources and local changes are
available through the following references:

| Component | Pinned source | Local changes |
| --- | --- | --- |
| Deno | https://github.com/denoland/deno at `abd22074e4` | `integration/deno.patch` and `integration/rt/` |
| Laufey | https://github.com/littledivy/laufey at `1fe8787` | `integration/laufey.patch` and `integration/native/` |
| Rust dependencies | Cargo.lock and integration/deno.Cargo.lock | Locked dependency versions |

The executable includes Rusty V8 and other transitive Deno dependencies. Preserve
their upstream license notices when redistributing builds. System WebView and
system CEF are supplied separately by the operating system. The Linux CEF build
adapts the CMake/ABI-check approach from the local Songjian reference project.
