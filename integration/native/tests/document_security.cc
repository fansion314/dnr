#include "../document_security.h"
#include <cassert>

int main() {
  using namespace dnr_document;
  assert(Configure(1, {"http://127.0.0.1:1234"}));
  assert(!Configure(1, {"https://evil.invalid"}));
  assert(!BeginNavigation(1, "http://127.0.0.1:12345/"));
  assert(!BeginNavigation(1, "http://127.0.0.1:1234.evil.invalid/"));
  assert(!BeginNavigation(1, "http://127.0.0.1:1234@evil.invalid/"));
  assert(BeginNavigation(1, "http://127.0.0.1:1234/index.html"));
  auto first = *Snapshot(1);
  assert(!AllowNavigation(1, first.url, false));
  assert(AllowNavigation(1, first.url, true));
  assert(!AllowNavigation(1, first.url, true));
  assert(AllowNavigation(1, first.url + "#vault", true));
  assert(!RegisterCall(1, 1, false, first.url, first.bridge_token));
  assert(!RegisterCall(1, 1, true, "https://evil.invalid/", first.bridge_token));
  assert(!RegisterCall(1, 1, true, first.url, "forged"));
  const auto call = RegisterCall(1, 1, true, first.url, first.bridge_token);
  assert(call && Lookup(call)->protected_document && Current(1, first.generation));
  assert(!BeginNavigation(1, first.url));
  Close(1);
  assert(!Current(1, first.generation));
  assert(Configure(1, {"http://127.0.0.1:1234"}));
  assert(BeginNavigation(1, first.url));
  assert(!RegisterCall(1, 1, true, first.url, first.bridge_token));
  assert(!Take(call));
  const auto a = RegisterCall(2, 1, true, "", "");
  const auto b = RegisterCall(3, 1, true, "", "");
  assert(a != b && Lookup(a)->window_id == 2 && Lookup(b)->window_id == 3);
  auto second = *Snapshot(1);
  for (int i = 1; i <= 128; ++i) assert(RegisterCall(1, i, true, second.url, second.bridge_token));
  assert(!RegisterCall(1, 129, true, second.url, second.bridge_token));
  Close(1);
  assert(!Current(1, second.generation));
}
