use std::hint;
use std::num::NonZeroU64;

use once_cell::sync::Lazy;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, SetWaitableTimer,
    TIMER_ALL_ACCESS,
};

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

pub fn sleep_until(target: Timestamp) {
    const MIN_SPIN_PERIOD: u64 = 500_000;
    let mut now = timestamp_now();

    let timer = unsafe {
        CreateWaitableTimerExW(
            None,
            None,
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS.0,
        )
    }
        .unwrap();

    eprintln!("LFX2 Sleep: {}us", target.saturating_sub(now) / 1000);

    while now + MIN_SPIN_PERIOD < target {
        let sleep_duration = -((target - now - MIN_SPIN_PERIOD) as i64) / 100;
        unsafe {
            SetWaitableTimer(timer, &sleep_duration, 0, None, None, false);
        }
        now = timestamp_now();
    }

    while now < target {
        hint::spin_loop();
        now = timestamp_now();
    }

    unsafe { CloseHandle(timer) }.unwrap();
}