//! Host cache adapter. Ordinary disk scripts never install a persistent context.
use deno_core::{ModuleSpecifier, v8};
use deno_runtime::code_cache::{CodeCache, CodeCacheType};
use dnr_package::persistent::{CompileCache, Generation, hash};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

struct Context {
    generation: Arc<Generation>,
    cache: Mutex<Option<Arc<CompileCache>>>,
    code: bool,
    emit: bool,
}
static CONTEXT: OnceLock<Context> = OnceLock::new();
static CODE_HITS: AtomicU64 = AtomicU64::new(0);
static CODE_MISSES: AtomicU64 = AtomicU64::new(0);
static EMIT_HITS: AtomicU64 = AtomicU64::new(0);
static EMIT_MISSES: AtomicU64 = AtomicU64::new(0);

pub fn init(generation: Arc<Generation>, code: bool, emit: bool) {
    let _ = CONTEXT.set(Context {
        generation,
        cache: Mutex::new(None),
        code,
        emit,
    });
}
fn cache() -> Option<Arc<CompileCache>> {
    let context = CONTEXT.get()?;
    let mut slot = context.cache.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        let fingerprint = format!(
            "dnr3:{}:{}:{}:{}:{}",
            env!("DNR_CACHE_BUILD_ID"),
            v8::V8::get_version(),
            v8::script_compiler::cached_data_version_tag(),
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        *slot = context.generation.compile(&fingerprint).ok().map(Arc::new);
    }
    slot.clone()
}
pub fn code_enabled() -> bool {
    CONTEXT.get().is_some_and(|c| c.code)
}
fn stable(specifier: &ModuleSpecifier) -> bool {
    matches!(specifier.scheme(), "file" | "ext" | "node")
}
pub struct DnrCodeCache;

/// Keep dnr's continuously writable cache outside Deno's standalone strategy.
/// The unmodified upstream implementation still owns standalone cache behavior.
pub enum RuntimeCodeCache {
    Dnr(DnrCodeCache),
    Standalone(crate::code_cache::DenoCompileCodeCache),
}
impl RuntimeCodeCache {
    pub fn for_dnr() -> Self {
        Self::Dnr(DnrCodeCache)
    }
    pub fn new(path: std::path::PathBuf, key: u64) -> Self {
        Self::Standalone(crate::code_cache::DenoCompileCodeCache::new(path, key))
    }
    pub fn enabled(&self) -> bool {
        match self {
            Self::Dnr(_) => true,
            Self::Standalone(cache) => cache.enabled(),
        }
    }
    pub fn for_deno_core(self: Arc<Self>) -> Arc<dyn CodeCache> {
        self
    }
}
impl CodeCache for RuntimeCodeCache {
    fn get_sync(
        &self,
        specifier: &ModuleSpecifier,
        kind: CodeCacheType,
        hash: u64,
    ) -> Option<Vec<u8>> {
        match self {
            Self::Dnr(cache) => cache.get_sync(specifier, kind, hash),
            Self::Standalone(cache) => cache.get_sync(specifier, kind, hash),
        }
    }
    fn set_sync(&self, specifier: ModuleSpecifier, kind: CodeCacheType, hash: u64, bytes: &[u8]) {
        match self {
            Self::Dnr(cache) => cache.set_sync(specifier, kind, hash, bytes),
            Self::Standalone(cache) => cache.set_sync(specifier, kind, hash, bytes),
        }
    }
}
impl CodeCache for DnrCodeCache {
    fn get_sync(
        &self,
        specifier: &ModuleSpecifier,
        kind: CodeCacheType,
        source_hash: u64,
    ) -> Option<Vec<u8>> {
        if !code_enabled() || !stable(specifier) {
            return None;
        }
        let value = cache()?.get(
            "code",
            &format!("{kind:?}:{specifier}"),
            &source_hash.to_string(),
        );
        if value.is_some() {
            CODE_HITS.fetch_add(1, Ordering::Relaxed);
        } else {
            CODE_MISSES.fetch_add(1, Ordering::Relaxed);
        }
        value
    }
    fn set_sync(
        &self,
        specifier: ModuleSpecifier,
        kind: CodeCacheType,
        source_hash: u64,
        bytes: &[u8],
    ) {
        if code_enabled()
            && stable(&specifier)
            && let Some(cache) = cache()
        {
            let _ = profile("code-write", || {
                cache.set(
                    "code",
                    &format!("{kind:?}:{specifier}"),
                    &source_hash.to_string(),
                    bytes,
                )
            });
        }
    }
}
pub fn emit_key(specifier: &ModuleSpecifier, source: &str, options: &str) -> Option<String> {
    if !CONTEXT.get().is_some_and(|c| c.emit) || !stable(specifier) {
        return None;
    }
    Some(hash(format!("{options}\0{source}").as_bytes()))
}
pub fn get_emit(specifier: &ModuleSpecifier, key: &str) -> Option<String> {
    let value = cache()?
        .get("emit", specifier.as_str(), key)
        .and_then(|v| String::from_utf8(v).ok());
    if value.is_some() {
        EMIT_HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        EMIT_MISSES.fetch_add(1, Ordering::Relaxed);
    }
    value
}
pub fn set_emit(specifier: &ModuleSpecifier, key: &str, text: &str) {
    if let Some(cache) = cache() {
        let _ = profile("emit-write", || {
            cache.set("emit", specifier.as_str(), key, text.as_bytes())
        });
    }
}
pub fn finish() {
    if let Some(c) = CONTEXT.get() {
        c.cache.lock().unwrap_or_else(|e| e.into_inner()).take();
        c.generation.finish();
        if std::env::var_os("DNR_CACHE_STATS").is_some() {
            eprintln!(
                "DNR_CACHE_STATS code_hits={} code_misses={} emit_hits={} emit_misses={}",
                CODE_HITS.load(Ordering::Relaxed),
                CODE_MISSES.load(Ordering::Relaxed),
                EMIT_HITS.load(Ordering::Relaxed),
                EMIT_MISSES.load(Ordering::Relaxed)
            );
        }
    }
}

pub fn profile<T>(kind: &'static str, f: impl FnOnce() -> T) -> T {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("DNR_PROFILE").is_some()) {
        let start = std::time::Instant::now();
        let result = f();
        let ns = start.elapsed().as_nanos();
        eprintln!("DNR_PROFILE kind={kind} ns={ns}");
        result
    } else {
        f()
    }
}
