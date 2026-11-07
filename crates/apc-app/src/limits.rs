//! Process limits the app needs raised.
//!
//! An app started from the Dock or Finder on macOS may keep only 256 files open at once. Playback
//! holds each recording and sound it plays open, and SQLite needs three files for each episode
//! (the file, its log and its index). A long review session of a scene full of sounds can reach
//! the limit, and then even copying a package fails ("unable to open database"). The hard limit
//! is far higher, so the soft one is raised to it (D159).

/// What the soft limit is raised to at most: macOS refuses more than `OPEN_MAX` for some calls.
#[cfg(unix)]
const WANTED: u64 = 10_240;

/// Raises the soft limit on open files towards the hard limit. Never lowers it, never fails.
// `rlim_t` is a u64 on macOS and Linux but not everywhere
#[allow(clippy::unnecessary_cast)]
pub fn raise_open_files() {
    #[cfg(unix)]
    {
        let mut lim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: getrlimit and setrlimit only read and write the struct passed to them.
        unsafe {
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
                return;
            }
            let want = (WANTED as libc::rlim_t).min(lim.rlim_max);
            if lim.rlim_cur < want {
                lim.rlim_cur = want;
                let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &lim);
            }
        }
    }
}

/// The soft limit on open files now (tests and diagnostics).
#[allow(clippy::unnecessary_cast)]
pub fn open_files_limit() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut lim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: as above.
        let ok = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } == 0;
        ok.then_some(lim.rlim_cur as u64)
    }
    #[cfg(not(unix))]
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_soft_limit_only_goes_up() {
        let before = open_files_limit();
        raise_open_files();
        let after = open_files_limit();
        if let (Some(b), Some(a)) = (before, after) {
            assert!(a >= b, "{b} -> {a}");
            assert!(a >= b.min(WANTED_FOR_TEST), "{a}");
        }
    }

    #[cfg(unix)]
    const WANTED_FOR_TEST: u64 = 256;
    #[cfg(not(unix))]
    const WANTED_FOR_TEST: u64 = 0;
}
