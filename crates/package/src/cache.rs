//! Per-entry single-flight reads and a byte-budgeted, second-chance CLOCK.
use crate::{CacheStats, DEFAULT_CACHE_BYTES};
use anyhow::{Result, anyhow};
use std::{
    collections::VecDeque,
    fmt,
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

type Bytes = Arc<[u8]>;
type SharedResult = std::result::Result<Bytes, SharedError>;

#[derive(Clone, Debug)]
struct SharedError(Arc<anyhow::Error>);
impl fmt::Display for SharedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for SharedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

// User/decoder code is never called under these locks. Recovering poison also
// lets the leader's unwind guard wake all waiters after an unexpected panic.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
struct Flight {
    result: Mutex<Option<SharedResult>>,
    ready: Condvar,
}
impl Flight {
    fn wait(&self) -> Result<Bytes> {
        let mut result = lock(&self.result);
        while result.is_none() {
            result = self.ready.wait(result).unwrap_or_else(|e| e.into_inner());
        }
        result.as_ref().unwrap().clone().map_err(anyhow::Error::new)
    }
    fn finish(&self, result: SharedResult) {
        *lock(&self.result) = Some(result);
        self.ready.notify_all();
    }
}

#[derive(Default)]
enum Value {
    #[default]
    Empty,
    Loading(Arc<Flight>),
    Ready {
        bytes: Bytes,
        referenced: bool,
    },
}
#[derive(Default)]
struct Entry {
    value: Value,
    hits: u64,
    decompressions: u64,
    coalesced: u64,
}
struct Slot {
    size: u64,
    entry: Mutex<Entry>,
}
#[derive(Default)]
struct Clock {
    queue: VecDeque<usize>,
    resident_bytes: usize,
}

pub(crate) struct Cache {
    slots: Box<[Slot]>,
    clock: Mutex<Clock>,
    budget: usize,
    limiter: Limiter,
}
impl Cache {
    pub(crate) fn new(sizes: Vec<u64>, budget: usize) -> Self {
        let parallelism = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8);
        Self {
            slots: sizes
                .into_iter()
                .map(|size| Slot {
                    size,
                    entry: Mutex::default(),
                })
                .collect(),
            clock: Mutex::default(),
            budget,
            limiter: Limiter::new(parallelism, DEFAULT_CACHE_BYTES),
        }
    }

    pub(crate) fn read(&self, index: usize, load: impl FnOnce() -> Result<Bytes>) -> Result<Bytes> {
        let slot = self
            .slots
            .get(index)
            .ok_or_else(|| anyhow!("invalid ZIP index: {index}"))?;
        let mut entry = lock(&slot.entry);
        match &mut entry.value {
            Value::Ready { bytes, referenced } => {
                *referenced = true;
                let bytes = bytes.clone();
                entry.hits += 1;
                return Ok(bytes);
            }
            Value::Loading(flight) => {
                let flight = flight.clone();
                entry.coalesced += 1;
                drop(entry);
                return flight.wait();
            }
            Value::Empty => {}
        }
        let flight = Arc::new(Flight::default());
        entry.value = Value::Loading(flight.clone());
        drop(entry);
        let leader = Leader {
            cache: self,
            index,
            flight,
            finished: false,
        };
        match usize::try_from(slot.size) {
            Ok(size) => {
                let _permit = self.limiter.acquire(size);
                leader.finish(load())
            }
            Err(_) => leader.finish(Err(anyhow!("file too large"))),
        }
    }

    fn complete(&self, index: usize, flight: &Arc<Flight>, result: SharedResult) {
        let mut retired = Vec::new();
        // Lock order: CLOCK -> slot -> flight. Readers release their slot
        // before waiting or acquiring CLOCK; no decoding under any cache lock.
        let mut clock = lock(&self.clock);
        let mut resident = false;
        if let Ok(bytes) = &result
            && bytes.len() <= self.budget
        {
            // Bound work even when concurrent hits keep marking candidates.
            let mut scans = clock.queue.len().saturating_mul(2);
            while clock.resident_bytes > self.budget - bytes.len() && scans > 0 {
                scans -= 1;
                let victim = clock.queue.pop_front().unwrap();
                let mut entry = lock(&self.slots[victim].entry);
                if let Value::Ready { referenced, .. } = &mut entry.value
                    && *referenced
                {
                    *referenced = false;
                    clock.queue.push_back(victim);
                    continue;
                }
                if let Value::Ready { bytes, .. } = std::mem::take(&mut entry.value) {
                    clock.resident_bytes -= bytes.len();
                    retired.push(bytes);
                }
            }
            resident = clock.resident_bytes <= self.budget - bytes.len();
        }
        let mut entry = lock(&self.slots[index].entry);
        if matches!(&entry.value, Value::Loading(current) if Arc::ptr_eq(current, flight)) {
            if let Ok(bytes) = &result {
                entry.decompressions += 1;
                if resident {
                    clock.resident_bytes += bytes.len();
                    clock.queue.push_back(index);
                    entry.value = Value::Ready {
                        bytes: bytes.clone(),
                        referenced: true,
                    };
                } else {
                    entry.value = Value::Empty;
                }
            } else {
                entry.value = Value::Empty;
            }
            // Waiters own the Flight, including for uncached results and errors.
            flight.finish(result);
        }
        drop(entry);
        drop(clock);
        drop(retired); // Potentially large allocations are freed outside locks.
    }

    pub(crate) fn stats(&self) -> CacheStats {
        let clock = lock(&self.clock);
        let mut stats = CacheStats {
            resident_bytes: clock.resident_bytes,
            ..CacheStats::default()
        };
        for slot in &self.slots {
            let entry = lock(&slot.entry);
            stats.hits += entry.hits;
            stats.decompressions += entry.decompressions;
            stats.coalesced += entry.coalesced;
        }
        stats
    }
}

struct Leader<'a> {
    cache: &'a Cache,
    index: usize,
    flight: Arc<Flight>,
    finished: bool,
}
impl Leader<'_> {
    fn finish(mut self, result: Result<Bytes>) -> Result<Bytes> {
        let result = result.map_err(|e| SharedError(Arc::new(e)));
        self.cache
            .complete(self.index, &self.flight, result.clone());
        self.finished = true;
        result.map_err(anyhow::Error::new)
    }
}
impl Drop for Leader<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.cache.complete(
                self.index,
                &self.flight,
                Err(SharedError(Arc::new(anyhow!("ZIP load interrupted")))),
            );
        }
    }
}

#[derive(Default)]
struct InFlight {
    count: usize,
    bytes: usize,
}
struct Limiter {
    state: Mutex<InFlight>,
    changed: Condvar,
    max_count: usize,
    max_bytes: usize,
}
impl Limiter {
    fn new(max_count: usize, max_bytes: usize) -> Self {
        Self {
            state: Mutex::default(),
            changed: Condvar::new(),
            max_count,
            max_bytes,
        }
    }
    fn acquire(&self, bytes: usize) -> Permit<'_> {
        let mut state = lock(&self.state);
        // A file larger than the byte limit runs alone, rather than waiting forever.
        while state.count != 0
            && (state.count >= self.max_count
                || state.bytes > self.max_bytes
                || bytes > self.max_bytes.saturating_sub(state.bytes))
        {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.count += 1;
        state.bytes += bytes;
        Permit {
            limiter: self,
            bytes,
        }
    }
}
struct Permit<'a> {
    limiter: &'a Limiter,
    bytes: usize,
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = lock(&self.limiter.state);
        state.count -= 1;
        state.bytes -= self.bytes;
        self.limiter.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn distinct_loads_and_resident_hits_progress_independently() {
        let mut cache = Cache::new(vec![4; 3], 12);
        cache.limiter = Limiter::new(2, 12);
        cache.read(2, || Ok(Arc::from(&b"hot!"[..]))).unwrap();
        thread::scope(|scope| {
            let (started, progress) = mpsc::channel();
            let mut releases = Vec::new();
            let mut workers = Vec::new();
            for index in 0..2 {
                let cache = &cache;
                let started = started.clone();
                let (release, resume) = mpsc::channel();
                releases.push(release);
                workers.push(scope.spawn(move || {
                    cache.read(index, || {
                        started.send(index)?;
                        resume.recv_timeout(TIMEOUT)?;
                        Ok(Arc::from([index as u8; 4]))
                    })
                }));
            }
            // Both decoders must enter before either one is allowed to finish.
            progress.recv_timeout(TIMEOUT).unwrap();
            progress.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(
                &*cache.read(2, || panic!("resident entry decoded")).unwrap(),
                b"hot!"
            );
            for release in releases {
                release.send(()).unwrap();
            }
            for worker in workers {
                worker.join().unwrap().unwrap();
            }
        });
        assert_eq!(cache.stats().decompressions, 3);
    }

    #[test]
    fn one_flight_shares_uncached_results_errors_and_unwind_then_retries() {
        for budget in [0, 4] {
            for outcome in 0..3 {
                let cache = Cache::new(vec![4], budget);
                thread::scope(|scope| {
                    let (started, progress) = mpsc::channel();
                    let (release, resume) = mpsc::channel();
                    let c = &cache;
                    let leader = scope.spawn(move || {
                        c.read(0, || {
                            started.send(())?;
                            resume.recv_timeout(TIMEOUT)?;
                            match outcome {
                                0 => Ok(Arc::from(&b"data"[..])),
                                1 => Err(anyhow!("retryable read failure")),
                                _ => panic!("decoder panic"),
                            }
                        })
                    });
                    progress.recv_timeout(TIMEOUT).unwrap();
                    let waiters: Vec<_> = (0..4)
                        .map(|_| scope.spawn(|| cache.read(0, || panic!("duplicate decoder"))))
                        .collect();
                    let deadline = Instant::now() + TIMEOUT;
                    while cache.stats().coalesced != 4 {
                        assert!(Instant::now() < deadline, "waiters failed to join flight");
                        thread::yield_now();
                    }
                    release.send(()).unwrap();
                    let first = leader.join();
                    for waiter in waiters {
                        let result = waiter.join().unwrap();
                        if outcome == 0 {
                            assert!(Arc::ptr_eq(
                                result.as_ref().unwrap(),
                                first.as_ref().unwrap().as_ref().unwrap()
                            ));
                        } else {
                            let message = result.unwrap_err().to_string();
                            assert!(message.contains(if outcome == 1 {
                                "retryable"
                            } else {
                                "interrupted"
                            }));
                        }
                    }
                    if outcome == 2 {
                        assert!(first.is_err());
                    }
                });
                assert_eq!(
                    cache.stats().resident_bytes,
                    if outcome == 0 { budget } else { 0 }
                );
                // Failures and uncached successes do not permanently occupy a slot.
                assert_eq!(
                    &*cache.read(0, || Ok(Arc::from(&b"data"[..]))).unwrap(),
                    b"data"
                );
            }
        }
    }

    #[test]
    fn clock_gives_referenced_entries_a_second_chance() {
        let cache = Cache::new(vec![4; 5], 12);
        for index in 0..4 {
            cache
                .read(index, || Ok(Arc::from([index as u8; 4])))
                .unwrap();
        }
        // Inserting 3 sweeps and clears the bits on 0,1,2, then evicts 0.
        cache.read(1, || panic!("1 should be resident")).unwrap();
        cache.read(4, || Ok(Arc::from([4; 4]))).unwrap();
        cache
            .read(1, || panic!("referenced 1 should survive"))
            .unwrap();
        let stats = cache.stats();
        assert_eq!(stats.resident_bytes, 12);
        assert_eq!(stats.decompressions, 5);
        assert!(matches!(lock(&cache.slots[2].entry).value, Value::Empty));
    }

    #[test]
    fn limiter_bounds_count_and_bytes_and_allows_oversized_files_alone() {
        let limiter = Limiter::new(3, 10);
        thread::scope(|scope| {
            for size in [0, 1, 4, 7, 11, 20, 0, 6] {
                let limiter = &limiter;
                scope.spawn(move || {
                    for _ in 0..50 {
                        let _permit = limiter.acquire(size);
                        let state = lock(&limiter.state);
                        assert!(state.count <= 3);
                        assert!(state.bytes <= 10 || state.count == 1);
                        drop(state);
                        thread::yield_now();
                    }
                });
            }
        });
        assert_eq!(lock(&limiter.state).count, 0);
        assert_eq!(lock(&limiter.state).bytes, 0);
    }
}
