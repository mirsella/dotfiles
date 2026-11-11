//! Helpers shared by the unit tests and the integration tests under `tests/`.

use std::{
    fs,
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::Path,
    process::{Command, Stdio},
};

/// Create an executable that can be run straight away.
///
/// Tests run on parallel threads, and many of them spawn children. A child
/// forked while this process has a file open for writing keeps that handle
/// until it execs, and the kernel refuses to run a file anyone can still write
/// (`ETXTBSY`). A separate process writes the file, so no fork here can ever
/// hold its write handle.
pub fn executable(path: &Path, contents: impl AsRef<[u8]>) {
    // Nix supplies its sandbox shell; native Linux builds use /bin/sh.
    let shell = option_env!("HOST_TOOLS_TEST_SHELL").unwrap_or("/bin/sh");
    let mut writer = Command::new(shell)
        .args(["-c", r#"cat > "$1""#, "write-executable"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = writer.stdin.take().unwrap();
    input.write_all(contents.as_ref()).unwrap();
    drop(input);
    assert!(
        writer.wait().unwrap().success(),
        "writing {} failed",
        path.display()
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
