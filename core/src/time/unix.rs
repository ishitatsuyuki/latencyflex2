use crate::Timestamp;
use nix::libc::{clock_nanosleep, prctl, PR_SET_TIMERSLACK};
use nix::sys::time::{TimeSpec, TimeValLike};
use nix::time::{clock_gettime, ClockId};
use std::ptr;
use std::sync::Once;

pub fn timestamp_now() -> Timestamp {
    let ts = clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap();
    ts.num_nanoseconds() as _
}

pub fn sleep_until(target: Timestamp) {
    static SET_TIMERSLACK: Once = Once::new();

    SET_TIMERSLACK.call_once(|| unsafe {
        prctl(PR_SET_TIMERSLACK, 1);
    });

    let ts = TimeSpec::nanoseconds(target as i64);
    unsafe {
        clock_nanosleep(
            ClockId::CLOCK_MONOTONIC.into(),
            nix::libc::TIMER_ABSTIME,
            ts.as_ref(),
            ptr::null_mut(),
        );
    }
}
