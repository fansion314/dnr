// DNR optional document boundary. All URLs passed here must be absolute URLs
// supplied by the native navigation API, never by the web page's call arguments.
#ifndef DNR_DOCUMENT_SECURITY_H
#define DNR_DOCUMENT_SECURITY_H

#include <atomic>
#include <cstdint>
#include <map>
#include <mutex>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>
#include <sys/random.h>
#include <unistd.h>

namespace dnr_document {

struct Document {
  uint64_t generation = 0;
  std::vector<std::string> origins;
  std::string url;
  std::string bridge_token;  // native-generated; only injected into a private world
  bool initial_navigation_pending = false;
};

struct Call {
  uint32_t window_id = 0;
  uint64_t page_call_id = 0;
  uint64_t generation = 0;
  std::string url;
  bool protected_document = false;
  bool main_frame = false;
};

inline std::mutex mutex;
inline std::map<uint32_t, Document> documents;
inline std::map<uint64_t, Call> calls;
inline std::set<uint32_t> closed_windows;
inline uint64_t next_generation = 1;
inline uint64_t next_call = 1;

inline std::string WithoutFragment(const std::string& url) {
  return url.substr(0, url.find('#'));
}

// Exact authority boundary, not a hostname prefix. The allowed origins are
// canonicalized and validated by the Rust API before reaching the backend.
inline bool OriginAllowed(const std::string& url,
                          const std::vector<std::string>& origins) {
  if (url.find_first_of("\\\r\n\t") != std::string::npos) return false;
  for (const auto& origin : origins) {
    if (url == origin || url.rfind(origin + "/", 0) == 0 ||
        url.rfind(origin + "?", 0) == 0 || url.rfind(origin + "#", 0) == 0) {
      return true;
    }
  }
  return false;
}

inline bool Configure(uint32_t window_id,
                      const std::vector<std::string>& origins) {
  if (origins.empty() || origins.size() > 32) return false;
  std::lock_guard<std::mutex> lock(mutex);
  if (documents.count(window_id)) return false;  // Immutable for this native window.
  Document doc;
  doc.origins = origins;
  closed_windows.erase(window_id);
  documents.emplace(window_id, std::move(doc));
  return true;
}

inline bool Protected(uint32_t window_id) {
  std::lock_guard<std::mutex> lock(mutex);
  return documents.count(window_id) != 0 || closed_windows.count(window_id) != 0;
}

// Only the privileged host can authorize a new document. Page-initiated full
// navigations/reloads and redirects are denied; in-document fragments are fine.
inline bool BeginNavigation(uint32_t window_id, const std::string& url) {
  std::lock_guard<std::mutex> lock(mutex);
  if (closed_windows.count(window_id)) return false;
  auto it = documents.find(window_id);
  if (it == documents.end()) return true;
  if (it->second.generation != 0 || !OriginAllowed(url, it->second.origins) || next_generation == UINT64_MAX) return false;
  unsigned char random[32];
  if (getentropy(random, sizeof(random)) != 0) return false;
  constexpr char hex[] = "0123456789abcdef";
  std::string token;
  token.reserve(64);
  for (unsigned char byte : random) {
    token += hex[byte >> 4]; token += hex[byte & 15];
  }
  auto& doc = it->second;
  doc.generation = next_generation++;
  doc.url = url;
  doc.bridge_token = std::move(token);
  doc.initial_navigation_pending = true;
  return true;
}

inline bool AllowNavigation(uint32_t window_id, const std::string& url,
                            bool main_frame) {
  std::lock_guard<std::mutex> lock(mutex);
  if (closed_windows.count(window_id)) return false;
  auto it = documents.find(window_id);
  if (it == documents.end()) return true;
  auto& doc = it->second;
  if (!main_frame || !OriginAllowed(url, doc.origins)) return false;
  if (doc.initial_navigation_pending && WithoutFragment(url) == WithoutFragment(doc.url)) {
    doc.initial_navigation_pending = false;
    return true;
  }
  // Only a changed fragment is an in-document navigation; same-URL reloads fail.
  if (url != doc.url && WithoutFragment(url) == WithoutFragment(doc.url)) {
    doc.url = url;
    return true;
  }
  return false;
}

inline std::optional<Document> Snapshot(uint32_t window_id) {
  std::lock_guard<std::mutex> lock(mutex);
  auto it = documents.find(window_id);
  if (it == documents.end()) return std::nullopt;
  return it->second;
}

inline bool Current(uint32_t window_id, uint64_t generation) {
  std::lock_guard<std::mutex> lock(mutex);
  auto it = documents.find(window_id);
  return generation != 0 && it != documents.end() && it->second.generation == generation;
}

// token is supplied by the native/private-world bridge, not the public page API.
// Passing a native frame's URL is additionally required for direct native bridges.
inline uint64_t RegisterCall(uint32_t window_id, uint64_t page_call_id,
                            bool main_frame, const std::string& native_url,
                            const std::string& token) {
  std::lock_guard<std::mutex> lock(mutex);
  if (window_id == 0 || page_call_id == 0 || page_call_id > 9007199254740991ULL || closed_windows.count(window_id)) return 0;
  Call call{window_id, page_call_id, 0, native_url, false, main_frame};
  auto it = documents.find(window_id);
  if (it != documents.end()) {
    const auto& doc = it->second;
    if (!main_frame || doc.generation == 0 || token != doc.bridge_token ||
        !OriginAllowed(native_url, doc.origins)) return 0;
    size_t pending = 0;
    for (const auto& [id, item] : calls) {
      if (item.window_id == window_id && item.generation == doc.generation) ++pending;
    }
    if (pending >= 128) return 0;
    call.generation = doc.generation;
    call.protected_document = true;
  }
  if (next_call == UINT64_MAX) return 0;
  const uint64_t id = next_call++;
  calls.emplace(id, std::move(call));
  return id;
}

inline std::optional<Call> Lookup(uint64_t id) {
  std::lock_guard<std::mutex> lock(mutex);
  auto it = calls.find(id);
  return it == calls.end() ? std::nullopt : std::optional<Call>(it->second);
}

inline std::optional<Call> Take(uint64_t id) {
  std::lock_guard<std::mutex> lock(mutex);
  auto node = calls.extract(id);
  if (node.empty()) return std::nullopt;
  return std::move(node.mapped());
}

inline void Invalidate(uint32_t window_id) {
  std::lock_guard<std::mutex> lock(mutex);
  auto doc = documents.find(window_id);
  if (doc != documents.end()) {
    doc->second.generation = 0;
    doc->second.bridge_token.clear();
    doc->second.initial_navigation_pending = false;
  }
  for (auto it = calls.begin(); it != calls.end();) {
    if (it->second.window_id == window_id) it = calls.erase(it); else ++it;
  }
}

inline void Close(uint32_t window_id) {
  std::lock_guard<std::mutex> lock(mutex);
  closed_windows.insert(window_id);
  documents.erase(window_id);
  for (auto it = calls.begin(); it != calls.end();) {
    if (it->second.window_id == window_id) it = calls.erase(it); else ++it;
  }
}

}  // namespace dnr_document
#endif
