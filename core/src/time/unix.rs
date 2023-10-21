use crate::Timestamp;
use nix::libc::{clock_nanosleep, prctl, PR_SET_TIMERSLACK};
use nix::sys::time::{TimeSpec, TimeValLike};
use nix::time::{clock_gettime, ClockId};
#[cfg(feature = "vulkan")]
use spark::vk;
use std::ptr;
use std::sync::Once;

#[cfg(feature = "vulkan")]
pub const VULKAN_TIMESTAMP_DOMAIN: vk::TimeDomainEXT = vk::TimeDomainEXT::CLOCK_MONOTONIC;
#[cfg(feature = "vulkan")]
pub fn timestamp_from_vulkan(calibration: u64) -> u64 {
    calibration
}

pub fn now() -> Timestamp {
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
