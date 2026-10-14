use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Duration};

pub fn path(path: &Path) -> Result<&str> {
    path.to_str().context("path is not UTF-8")
}

pub fn output(program: &str, args: &[&str]) -> Result<String> {
    String::from_utf8(command_output(Command::new(program).args(args))?)
        .with_context(|| format!("{program} returned non-UTF-8 output"))
}

pub fn command_output(command: &mut Command) -> Result<Vec<u8>> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .with_context(|| format!("start {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

pub fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("start {program}"))?;
    anyhow::ensure!(status.success(), "{program} exited {status}");
    Ok(())
}

pub fn json(program: &str, args: &[&str]) -> Result<Value> {
    serde_json::from_str(&output(program, args)?).with_context(|| format!("parse {program} JSON"))
}

pub fn read(path: impl AsRef<Path>) -> Result<String> {
    let p = path.as_ref();
    fs::read_to_string(p).with_context(|| format!("read {}", p.display()))
}

pub fn agent(seconds: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(seconds)))
        .http_status_as_error(false)
        .build()
        .into()
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().context("checkpoint lacks parent")?)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

/// Wait on filesystem events, rechecking after installing the watch to close races.
pub fn wait_for_file<T>(
    path: &Path,
    timeout: Duration,
    mut read: impl FnMut() -> Result<Option<T>>,
) -> Result<T> {
    use std::{
        ffi::CString,
        io,
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        },
        time::Instant,
    };
    if let Some(value) = read()? {
        return Ok(value);
    }
    let parent = path.parent().context("Watched file needs a parent")?;
    fs::create_dir_all(parent)?;
    let raw = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
    if raw < 0 {
        return Err(io::Error::last_os_error()).context("Create readiness watch");
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let parent = CString::new(parent.as_os_str().as_bytes())?;
    if unsafe {
        libc::inotify_add_watch(
            fd.as_raw_fd(),
            parent.as_ptr(),
            libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE,
        )
    } < 0
    {
        return Err(io::Error::last_os_error()).context("Watch readiness directory");
    }
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = read()? {
            return Ok(value);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        anyhow::ensure!(
            !remaining.is_zero(),
            "Timed out waiting for {}",
            path.display()
        );
        let mut event = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let status = unsafe {
            libc::poll(
                &mut event,
                1,
                remaining.as_millis().clamp(1, i32::MAX as u128) as i32,
            )
        };
        if status < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error).context("Wait for readiness");
        }
        if status > 0 {
            let mut buffer = [0u8; 4096];
            if unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) } < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::WouldBlock {
                    return Err(error).context("Read readiness event");
                }
            }
        }
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    #[test]
    fn waits_for_atomic_creation_and_reports_timeout() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("runtime.json");
        let writer_path = path.clone();
        let writer = std::thread::spawn(move || {
            // Ensure the reader has to observe creation rather than a pre-existing file.
            std::thread::sleep(Duration::from_millis(30));
            atomic_write(&writer_path, b"ready").unwrap();
        });
        let result = wait_for_file(&path, Duration::from_secs(2), || match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        })?;
        writer.join().unwrap();
        assert_eq!(result, b"ready");
        assert!(
            wait_for_file::<()>(
                &root.path().join("missing"),
                Duration::from_millis(5),
                || Ok(None)
            )
            .is_err()
        );
        Ok(())
    }
}
