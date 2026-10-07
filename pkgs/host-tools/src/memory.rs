use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CString, OsStr},
    fs::{self, File},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::Path,
};

// Apply to individual executables, never to an entire terminal/server cgroup:
// builds launched inside a protected session must remain eligible OOM victims.
const INTERACTIVE_SCORE: i32 = -900;

fn desired_score(executable: &OsStr, current: i32) -> i32 {
    let name = executable.as_bytes();
    match name.strip_suffix(b" (deleted)").unwrap_or(name) {
        b"opencode" | b"rio" | b"wezterm-gui" => INTERACTIVE_SCORE,
        b"rustc" | b"cc1" | b"cc1plus" | b"clang" | b"clang++" | b"gcc" | b"g++" | b"ld"
        | b"ld.lld" | b"lld" | b"rust-lld" | b"wasm-ld" | b"wasm-opt" | b"wasm-bindgen" => {
            current.max(800)
        }
        b"cargo" => current.max(500),
        // Linux inherits oom_score_adj across fork AND exec. Reserve this score
        // for interactive executables, not their shells and build scripts.
        _ if current == INTERACTIVE_SCORE => 200,
        _ => current,
    }
}

fn apply_process(path: &Path, uid: u32) -> Result<()> {
    // A held proc directory refers to this process identity even if its PID is
    // later reused. Access through the fd rather than reopening /proc/<pid>.
    let directory = File::open(path)?;
    if directory.metadata()?.uid() != uid {
        return Ok(());
    }
    let mut pinned =
        std::path::PathBuf::from(format!("/proc/self/fd/{}/exe", directory.as_raw_fd()));
    let executable_path = fs::read_link(&pinned)?;
    let executable = executable_path
        .file_name()
        .context("missing executable name")?;
    pinned.set_file_name("oom_score_adj");
    let current = fs::read_to_string(&pinned)?
        .trim()
        .parse()
        .context("invalid oom_score_adj")?;
    let desired = desired_score(executable, current);
    if desired != current {
        fs::write(pinned, desired.to_string())?;
        println!(
            "OOM priority: pid={} executable={} {current} -> {desired}",
            path.file_name().context("missing PID")?.display(),
            executable.to_string_lossy()
        );
    }
    Ok(())
}

fn sweep(proc: &Path, uid: u32) -> Result<()> {
    for entry in fs::read_dir(proc)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_none_or(|name| name.parse::<u32>().is_err())
        {
            continue;
        }
        if let Err(error) = apply_process(&entry.path(), uid) {
            // A process can exit at any point during enumeration or /proc reads.
            if !error.downcast_ref::<io::Error>().is_some_and(|error| {
                matches!(error.raw_os_error(), Some(libc::ENOENT | libc::ESRCH))
            }) {
                return Err(error)
                    .with_context(|| format!("set OOM priority for {}", entry.path().display()));
            }
        }
    }
    Ok(())
}

fn ticker() -> Result<File> {
    let fd = unsafe { libc::timerfd_create(libc::CLOCK_MONOTONIC, libc::TFD_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error()).context("create OOM policy timer");
    }
    let timer = unsafe { File::from_raw_fd(fd) };
    let interval = libc::timespec {
        tv_sec: 0,
        tv_nsec: 250_000_000,
    };
    let spec = libc::itimerspec {
        it_interval: interval,
        it_value: interval,
    };
    if unsafe { libc::timerfd_settime(fd, 0, &spec, std::ptr::null_mut()) } != 0 {
        return Err(io::Error::last_os_error()).context("arm OOM policy timer");
    }
    Ok(timer)
}

pub fn run(user: &str) -> Result<()> {
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "OOM protection requires root"
    );
    let name = CString::new(user)?;
    // Resolve once before entering this single-threaded service loop.
    let account = unsafe { libc::getpwnam(name.as_ptr()) };
    ensure!(!account.is_null(), "Unknown OOM protection user: {user}");
    let uid = unsafe { (*account).pw_uid };
    ensure!(
        uid != 0,
        "OOM policy must target a non-root workstation user"
    );
    println!(
        "Protecting OpenCode/Rio/WezTerm for {user} (uid={uid}); interactive={INTERACTIVE_SCORE}, compilers=800, Cargo=500; interval=250ms"
    );
    let mut timer = ticker()?;
    loop {
        sweep(Path::new("/proc"), uid)?;
        timer.read_exact(&mut [0_u8; 8])?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn process(root: &Path, pid: &str, exe: &str, score: i32) -> Result<std::path::PathBuf> {
        let path = root.join(pid);
        fs::create_dir(&path)?;
        symlink(exe, path.join("exe"))?;
        fs::write(path.join("oom_score_adj"), score.to_string())?;
        Ok(path)
    }

    fn score(path: &Path) -> Result<i32> {
        Ok(fs::read_to_string(path.join("oom_score_adj"))?.parse()?)
    }

    #[test]
    fn protection_does_not_follow_exec_into_builds_or_browsers() -> Result<()> {
        let root = tempfile::tempdir()?;
        let parent = process(root.path(), "11", "/usr/bin/opencode", 200)?;
        let compiler = process(root.path(), "12", "/toolchain/bin/rustc", -900)?;
        let browser = process(root.path(), "13", "/usr/bin/helium", -900)?;
        let shell = process(root.path(), "14", "/usr/bin/nu", -900)?;
        let cargo = process(root.path(), "15", "/toolchain/bin/cargo", -900)?;
        let rio = process(root.path(), "16", "/usr/bin/rio", 200)?;
        let deleted = process(root.path(), "17", "/usr/bin/opencode (deleted)", 200)?;
        sweep(root.path(), unsafe { libc::getuid() })?;
        assert_eq!(score(&parent)?, -900);
        assert_eq!(score(&rio)?, -900);
        assert_eq!(score(&deleted)?, -900);
        assert_eq!(score(&compiler)?, 800);
        assert_eq!(score(&cargo)?, 500);
        assert_eq!(score(&browser)?, 200);
        assert_eq!(score(&shell)?, 200);
        // The same process can exec another binary without changing PID.
        fs::remove_file(parent.join("exe"))?;
        symlink("/usr/bin/bash", parent.join("exe"))?;
        sweep(root.path(), unsafe { libc::getuid() })?;
        assert_eq!(score(&parent)?, 200);
        Ok(())
    }

    #[test]
    fn unrelated_priorities_and_other_users_are_preserved() -> Result<()> {
        let root = tempfile::tempdir()?;
        let unrelated = process(root.path(), "21", "/usr/bin/browser", 300)?;
        let reserved = process(root.path(), "22", "/usr/bin/agent", -750)?;
        let urgent = process(root.path(), "23", "/usr/bin/rustc", 1000)?;
        let terminal = process(root.path(), "24", "/usr/bin/rio", 200)?;
        let unusual = process(root.path(), "25", "/usr/bin/tool", -900)?;
        fs::remove_file(unusual.join("exe"))?;
        symlink(OsStr::from_bytes(b"/usr/bin/tool\xff"), unusual.join("exe"))?;
        let uid = unsafe { libc::getuid() };
        sweep(root.path(), uid + 1)?;
        assert_eq!(score(&terminal)?, 200);
        assert_eq!(score(&unusual)?, -900);
        sweep(root.path(), uid)?;
        assert_eq!(score(&unrelated)?, 300);
        assert_eq!(score(&reserved)?, -750);
        assert_eq!(score(&urgent)?, 1000);
        assert_eq!(score(&terminal)?, -900);
        assert_eq!(score(&unusual)?, 200);
        Ok(())
    }

    #[test]
    fn exited_processes_are_benign_but_invalid_state_is_an_error() -> Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir(root.path().join("31"))?;
        sweep(root.path(), unsafe { libc::getuid() })?;
        let invalid = process(root.path(), "32", "/usr/bin/rio", 200)?;
        fs::write(invalid.join("oom_score_adj"), "invalid")?;
        assert!(sweep(root.path(), unsafe { libc::getuid() }).is_err());
        assert_eq!(desired_score(OsStr::new("opencode-helper"), 200), 200);
        Ok(())
    }
}
