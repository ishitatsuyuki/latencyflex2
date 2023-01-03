use crate::Timestamp;
use nix::libc::clock_nanosleep;
use nix::sys::time::{TimeSpec, TimeValLike};
use nix::time::{clock_gettime, ClockId};
use std::ptr;

pub fn timestamp_now() -> Timestamp {
    let ts = clock_gettime(ClockId::CLOCK_MONOTONIC_RAW).unwrap();
    ts.num_nanoseconds() as _
}

pub fn sleep_until(target: Timestamp) {
    let ts = TimeSpec::nanoseconds(target as i64);
    unsafe {
        clock_nanosleep(
            ClockId::CLOCK_MONOTONIC_RAW.into(),
            nix::libc::TIMER_ABSTIME,
            ts.as_ref(),
            ptr::null_mut(),
        );
    }
}
