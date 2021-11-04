//! [`LLVM` `8-bi-counters`](https://clang.llvm.org/docs/SanitizerCoverage.html#tracing-pcs-with-guards) runtime for `LibAFL`.
use alloc::vec::Vec;
use core::slice::from_raw_parts_mut;

pub static mut COUNTERS_MAPS: Vec<&'static mut [u8]> = Vec::new();

/// Initialize the sancov `8-bit-counters` - usually called by `llvm`.
///
/// # Safety
/// Set up our coverage maps.
#[no_mangle]
pub fn __sanitizer_cov_8bit_counters_init(start: *mut u8, stop: *mut u8) {
    unsafe { COUNTERS_MAPS.push(from_raw_parts_mut(start, stop.offset_from(start) as usize)) }
}