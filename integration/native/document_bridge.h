#ifndef DNR_DOCUMENT_BRIDGE_H
#define DNR_DOCUMENT_BRIDGE_H
#include <string>

// WebKitGTK's legacy message signal has no frame provenance. Protected windows
// therefore expose the native handler only in a private, top-frame-only world.
// The public bridge posts unprivileged messages; MessageEvent.source is checked
// in the isolated world before adding the native-generated document capability.
inline std::string DnrPrivateBridge(const std::string& token) {
  return R"JS((() => {
    const capability = ")JS" + token + R"JS(";
    window.addEventListener('message', event => {
      if (event.source !== window) return;
      const data = event.data;
      if (!data || data.__dnrCall !== true || !Number.isSafeInteger(data.callId) || data.callId < 1 ||
          typeof data.method !== 'string' || data.method.length > 1024 || !Array.isArray(data.args)) return;
      window.webkit.messageHandlers.laufey.postMessage(JSON.stringify({
        callId: data.callId, method: data.method, args: data.args, _dnrToken: capability,
      }));
    });
  })();)JS";
}

inline std::string DnrPublicPostMessage() {
  return "window.postMessage({__dnrCall:true,callId:callId,method:path.join('.'),args:processedArgs}, '*');";
}
#endif
