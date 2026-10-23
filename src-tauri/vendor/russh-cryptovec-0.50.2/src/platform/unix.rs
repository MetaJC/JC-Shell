use std::ffi::CStr;

use libc::c_void;

use super::MemoryLockError;

/// Unlock memory on drop for Unix-based systems.
pub fn munlock(ptr: *const u8, len: usize) -> Result<(), MemoryLockError> {
    unsafe {
        if libc::munlock(ptr as *const c_void, len) != 0 {
            return Err(MemoryLockError::new(get_libc_error("munlock")));
        }
    }
    Ok(())
}

pub fn mlock(ptr: *const u8, len: usize) -> Result<(), MemoryLockError> {
    unsafe {
        if libc::mlock(ptr as *const c_void, len) != 0 {
            return Err(MemoryLockError::new(get_libc_error("mlock")));
        }
    }
    Ok(())
}

pub fn memset(ptr: *mut u8, value: i32, size: usize) {
    unsafe {
        libc::memset(ptr as *mut c_void, value, size);
    }
}

unsafe fn get_libc_error(msg: &str) -> String {
    // iOS 与 macOS 同属 Apple 平台，libc 不提供 glibc 私有符号 `__errno_location`，
    // 需用 Apple 专用的 `__error()`。原 crate 只特判了 macos，漏掉 ios，导致 iOS 交叉编译失败。
    // 此处把 ios 一并并入（Linux 仍走上游的 `__errno_location`，桌面行为不变）。
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let errno = *libc::__error();
    // Android (bionic)：libc crate 未绑定 errno 取值符号。bionic 仅在 32 位 ABI 上导出
    // glibc 兼容别名 `__errno_location`，64 位（arm64/x86_64）只有 `__errno`——
    // 若引用 `__errno_location`，64 位设备/模拟器上 dlopen 会直接报 UnsatisfiedLinkError。
    // `__errno()` 自 API 1 起在全部 ABI 上导出，这里自行声明后调用，全平台可用。
    #[cfg(target_os = "android")]
    let errno = *{
        extern "C" {
            fn __errno() -> *mut libc::c_int;
        }
        __errno()
    };
    // Linux 等其余 Unix：使用 glibc 的 `__errno_location`（桌面行为不变）
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
    let errno = *libc::__errno_location();
    const ERRMAXLEN: usize = 255;
    const INVALID_ERR: &str = "Unknown";
    let mut errdesc = [0u8; ERRMAXLEN];
    let errdesc = if libc::strerror_r(errno, errdesc.as_mut_ptr() as _, ERRMAXLEN) == 0 {
        if let Some(nul_pos) = errdesc.iter().position(|b| *b == 0) {
            #[allow(clippy::indexing_slicing)]
            // safety: the position was just checked, so it is guaranteed to be in range
            CStr::from_bytes_with_nul(&errdesc[0..=nul_pos])
                .ok()
                .and_then(|msg| msg.to_str().ok())
                .unwrap_or(INVALID_ERR)
        } else {
            INVALID_ERR
        }
    } else {
        INVALID_ERR
    };
    // Note: if you get 'Cannot allocate memory (0xc)' here,
    // check if your RLIMIT_MEMLOCK (`ulimit -l`) is configured low!
    format!("{}: {} (0x{:x})", msg, errdesc, errno)
}
