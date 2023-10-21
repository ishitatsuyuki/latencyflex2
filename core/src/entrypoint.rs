use crate::time;
use crate::Timestamp;

#[no_mangle]
pub unsafe extern "C" fn lfx2TimestampNow() -> Timestamp {
    time::now()
}

#[cfg(target_os = "windows")]
#[no_mangle]
pub unsafe extern "C" fn lfx2TimestampFromQpc(qpc: u64) -> Timestamp {
    time::timestamp_from_qpc(qpc)
}

#[no_mangle]
pub unsafe extern "C" fn lfx2SleepUntil(target: Timestamp) {
    time::sleep_until(target)
}
