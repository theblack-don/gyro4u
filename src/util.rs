use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[inline]
pub fn mlock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[inline]
pub fn rlock<T>(m: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|e| e.into_inner())
}

#[inline]
pub fn wlock<T>(m: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(|e| e.into_inner())
}

pub struct RateLimiter {
    last: Option<Instant>,
    interval: Duration,
}

impl RateLimiter {
    pub fn new(interval: Duration) -> Self {
        Self {
            last: None,
            interval,
        }
    }

    pub fn ready(&mut self) -> bool {
        if self.last.is_none_or(|last| last.elapsed() >= self.interval) {
            self.last = Some(Instant::now());
            true
        } else {
            false
        }
    }
}

pub fn rate_limited_gate(last: &AtomicU64, interval: Duration) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let previous = last.load(Ordering::Relaxed);
    if now.saturating_sub(previous) >= interval.as_secs().max(1) {
        last.store(now, Ordering::Relaxed);
        true
    } else {
        false
    }
}

#[inline]
pub fn wait_cv<'a, T>(
    cv: &Condvar,
    guard: MutexGuard<'a, T>,
    timeout: Duration,
) -> (MutexGuard<'a, T>, bool) {
    match cv.wait_timeout(guard, timeout) {
        Ok((g, r)) => (g, r.timed_out()),
        Err(e) => (e.into_inner().0, false),
    }
}
