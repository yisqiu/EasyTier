//! Suspend-aware local cache time. Never serialize these values as wall time.

use std::time::{Duration, Instant, SystemTime};

pub(super) trait RouteClock: Send + Sync {
    fn instant(&self) -> Duration;
    fn wall_time(&self) -> SystemTime;
}

pub(super) struct SystemRouteClock {
    started: Instant,
    boot_started: Option<Duration>,
    last: std::sync::Mutex<(Duration, Instant)>,
}

impl Default for SystemRouteClock {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            boot_started: boot_time(),
            last: std::sync::Mutex::new((Duration::ZERO, Instant::now())),
        }
    }
}

impl RouteClock for SystemRouteClock {
    fn instant(&self) -> Duration {
        let native = self
            .boot_started
            .zip(boot_time())
            .and_then(|(start, now)| now.checked_sub(start));
        // Continue from the last reading if a native call fails after resume.
        let mut last = self.last.lock().unwrap();
        let fallback = if self.boot_started.is_some() {
            last.0 + last.1.elapsed()
        } else {
            self.started.elapsed()
        };
        last.0 = last.0.max(native.unwrap_or(fallback));
        last.1 = Instant::now();
        last.0
    }

    fn wall_time(&self) -> SystemTime {
        SystemTime::now()
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
))]
fn boot_time() -> Option<Duration> {
    use nix::libc;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let clock = libc::CLOCK_BOOTTIME;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let clock = libc::CLOCK_MONOTONIC;
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: value is a valid writable timespec; the selected clock has no
    // pointers or ownership requirements and includes suspended time.
    if unsafe { libc::clock_gettime(clock, &mut value) } != 0 {
        return None;
    }
    Some(Duration::new(
        value.tv_sec.try_into().ok()?,
        value.tv_nsec.try_into().ok()?,
    ))
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
)))]
fn boot_time() -> Option<Duration> {
    // Windows Instant uses QPC; other platforms retain the standard fallback.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cache_clock_is_nondecreasing() {
        let clock = SystemRouteClock::default();
        let first = clock.instant();
        let second = clock.instant();
        assert!(second >= first);
        assert!(
            clock
                .wall_time()
                .duration_since(SystemTime::UNIX_EPOCH)
                .is_ok()
        );
    }

    #[test]
    fn failed_native_read_continues_from_last_successful_reading() {
        let clock = SystemRouteClock {
            started: Instant::now(),
            boot_started: Some(Duration::MAX),
            last: std::sync::Mutex::new((
                Duration::from_secs(7200),
                Instant::now() - Duration::from_secs(5),
            )),
        };
        // A failed/invalid native reading after resume must not freeze at 7200s
        // while an Instant measured before suspend catches up.
        assert!(clock.instant() >= Duration::from_secs(7205));
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ))]
    #[test]
    fn suspend_aware_native_clock_is_available() {
        assert!(boot_time().is_some());
    }
}
