//! On macOS, files and directories evicted to iCloud Drive or another
//! File Provider are "dataless": the kernel downloads one the moment
//! anything reads it, and a `readdir` of an evicted package waits for
//! the download. A scan of a fresh install sat for minutes inside
//! `getdirentries64` on Playgrounds books and game saves that way. Spotlight,
//! Time Machine and every backup tool turn materialisation off for their
//! process with `setiopolicy_np`, and so does the walk: an evicted directory
//! then fails to list and is shown as not read, a downloaded one is counted.
//! The same call stops autofs trigger mounts from firing, so `/home` is
//! not mounted just by being looked at.

/// Turn off dataless materialisation and automount triggers for this
/// process. A no-op elsewhere than macOS.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub fn skip_dataless() {
    use std::os::raw::c_int;
    extern "C" {
        fn setiopolicy_np(iotype: c_int, scope: c_int, policy: c_int) -> c_int;
    }
    // sys/resource.h: IOPOL_SCOPE_PROCESS, IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES
    // with IOPOL_MATERIALIZE_DATALESS_FILES_OFF, IOPOL_TYPE_VFS_TRIGGER_RESOLVE
    // with IOPOL_VFS_TRIGGER_RESOLVE_OFF.
    const SCOPE_PROCESS: c_int = 0;
    const MATERIALIZE_DATALESS: c_int = 3;
    const MATERIALIZE_OFF: c_int = 1;
    const TRIGGER_RESOLVE: c_int = 5;
    const TRIGGER_RESOLVE_OFF: c_int = 1;
    // SAFETY: a libc call with three integer arguments and no pointers,
    // available since macOS 10.5. It cannot fail in a way that matters
    // here: an unknown policy is refused with EINVAL and the walk goes on
    // as before.
    unsafe {
        setiopolicy_np(MATERIALIZE_DATALESS, SCOPE_PROCESS, MATERIALIZE_OFF);
        setiopolicy_np(TRIGGER_RESOLVE, SCOPE_PROCESS, TRIGGER_RESOLVE_OFF);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn skip_dataless() {}
