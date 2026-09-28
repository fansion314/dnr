# Document authority and background security events

Applications handling secrets can opt into a native document boundary. This is
separate from DNR's process permissions: application host code still has full
permissions and must expose only narrowly validated operations.

```ts
const origin = `http://127.0.0.1:${server.addr.port}`;
const window = new Deno.BrowserWindow({
  documentPolicy: { allowedOrigins: [origin] },
});
window.bindWithContext("approve", async (context, requestId, accepted) => {
  if (!context.currentDocument) throw new Error("Document expired");
  return approvals.respond(requestId, accepted, context);
});
window.navigate(origin + "/");
```

`allowedOrigins` accepts canonical HTTP(S) origins, without credentials, paths,
queries or fragments. The policy is immutable for each native window. Protected
windows permit the host-authorized first document and in-document routing;
page-initiated replacement documents, redirects and child-window navigation are
denied. Host `navigate()` and `reload()` recreate the native document and revoke
the old one. Applications handling secrets must additionally supply a CSP with
`frame-src 'none'` and serve only their packaged assets. Same-origin JavaScript
shares the document's authority; native provenance is not an XSS sandbox.
Unprotected windows retain existing behavior.

`bindWithContext()` requires a document policy. Its first argument is created
from native provenance, never supplied by page arguments: `trusted`,
`mainFrame`, `url`, `origin`, `documentId`, `currentDocument` and `signal`.
Contexts are canonical per document, allowing an approval to be tied to the
exact document that presented it. Destruction, release, replacement and renderer
termination invalidate old authority. A callback finishing after revocation
cannot resolve into a replacement document. Applications must also recheck
account/session/key state immediately before their privileged operation.

WebKitGTK uses a top-frame private-world bridge because its public script-message
callback lacks sufficient frame provenance. The native-generated bridge token
is never exposed to the application page or in `BindingContext`. WKWebView and
CEF verify native frame identity and source URL. Native call IDs are globally
unique across windows, distinct from page-local IDs. Protected documents have a
128-call native pending limit; applications should impose tighter request limits.
CEF additionally checks the entered V8 context, rejecting a child that borrows
a binding function created in its parent's context.

Protected bindings suppress argument/result tracing and replace exceptions with
a generic error. `DNR_DOCUMENT_DIAGNOSTICS` is an opt-in native diagnostic that
prints only window IDs, policy booleans and load error codes, never URLs,
binding arguments, results or tokens. Do not enable it for ordinary releases.

## System monitoring

`Deno.desktop.startSystemMonitoring()` returns the actual `lock`, `suspend`,
`resume` and `idleTime` capabilities. Listen for `systemstatechange` on
`Deno.desktop`, query `getSystemIdleTime()` (seconds, or `null` if unavailable),
and call `stopSystemMonitoring()` at shutdown. Monitoring keeps the application
alive without an open window. The runtime does not decide whether an application
should lock; that remains the application's existing lock policy.

Security events are coalesced separately from ordinary input events and drained
first, so a full input queue cannot drop a lock/suspend event. macOS uses native
session/workspace notifications and CoreGraphics idle time. Linux uses session
screensaver and logind signals and X11 idle time. Missing buses/services are
reported as unavailable. XWayland activity is not advertised as whole-session
idle time. Applications must handle unsupported policies explicitly.

## Verification and limits

On 2026-09-28, native builds passed on macOS ARM64 and remote Arch x86_64. The
`examples/desktop/document-security.ts` fixture passed on WKWebView, WebKitGTK
and system CEF: foreign navigation, redirects, subframe calls, page release and
reconstruction, stale responses, and overlapping page-local IDs in two windows.
`examples/desktop/system-monitor.ts` passed on macOS and Linux WebKitGTK;
the remote KDE container reported only idle time because it has no physical
logind session. Physical lock/sleep delivery remains to be checked on complete
desktop sessions. The Rust regression for a saturated input queue passed.

The remote CEF fixture initially stalled even on the installed old runtime:
Chromium was waiting for the isolated KDE Secret Service's wallet creation.
Initializing an encrypted, disposable wallet resolved it. The final test uses
the normal system credential backend; no basic/plaintext-password switch is
included in this change. Tests use loopback content and no real vault.

The v0.5.1 candidate also passed workspace formatting, strict Clippy and normal
workspace tests, plus 20 explicit macOS runtime/package/native/cache tests.
Linux native-plugin tests were executed as an unprivileged user for the
read-only-directory case; running that case as container root correctly fails
its prerequisite check and is not counted as a pass.
