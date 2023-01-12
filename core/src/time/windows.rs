use std::hint;
use std::num::NonZeroU64;

use once_cell::sync::Lazy;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    CreateWaitableTimerExW, SetWaitableTimer, WaitForSingleObject,
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS,
};
use windows::Win32::System::WindowsProgramming::INFINITE;

use crate::Timestamp;

pub fn timestamp_from_qpc(qpc: u64) -> Timestamp {
    static QPF: Lazy<NonZeroU64> = Lazy::new(|| {
        let mut qpf = 0i64;
        unsafe {
            QueryPerformanceFrequency(&mut qpf);
        }
        NonZeroU64::new(qpf as u64).unwrap()
    });

    let denom = 1_000_000_000;
    let whole = qpc / QPF.get() * denom;
    let part = qpc % QPF.get() * denom / QPF.get();
    (whole + part) as _
}

pub fn timestamp_now() -> Timestamp {
    let mut qpc = 0i64;
    unsafe {
        QueryPerformanceCounter(&mut qpc);
    }
    timestamp_from_qpc(qpc as u64)
}

struct WaitableTimer(HANDLE);

impl WaitableTimer {
    fn new() -> WaitableTimer {
        WaitableTimer(
            unsafe {
                CreateWaitableTimerExW(
                    None,
                    None,
                    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                    TIMER_ALL_ACCESS.0,
                )
            }
            .unwrap(),
        )
    }
}

impl Drop for WaitableTimer {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

thread_local! {
    static TIMER: WaitableTimer = WaitableTimer::new();
}

pub fn sleep_until(target: Timestamp) {
    const MIN_SPIN_PERIOD: u64 = 500_000;
    let mut now = timestamp_now();

    while now + MIN_SPIN_PERIOD < target {
        let sleep_duration = -((target - now - MIN_SPIN_PERIOD) as i64 + 99) / 100;
        TIMER.with(|timer| unsafe {
            SetWaitableTimer(timer.0, &sleep_duration, 0, None, None, false)
                .ok()
                .unwrap();
            WaitForSingleObject(timer.0, INFINITE).ok().unwrap();
        });
        now = timestamp_now();
    }

    while now < target {
        hint::spin_loop();
        now = timestamp_now();
    }
}
