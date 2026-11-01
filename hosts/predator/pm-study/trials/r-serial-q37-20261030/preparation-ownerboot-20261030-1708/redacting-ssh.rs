use std::{
    env,
    io::{self, Read, Write},
    process::{Command, Stdio},
    thread,
};

fn clean(mut bytes: Vec<u8>, secrets: &[String]) -> Vec<u8> {
    for secret in secrets {
        let needle = secret.as_bytes();
        assert!(!needle.is_empty());
        let mut output = Vec::new();
        let mut start = 0;
        while let Some(at) = bytes[start..]
            .windows(needle.len())
            .position(|part| part == needle)
        {
            output.extend_from_slice(&bytes[start..start + at]);
            output.extend_from_slice(b"[REDACTED_NOTIFICATION_PRIVATE_VALUE]");
            start += at + needle.len();
        }
        output.extend_from_slice(&bytes[start..]);
        bytes = output;
    }
    bytes
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let secrets = [env::var("TgToken")?, env::var("TgId")?];
    assert!(secrets.iter().all(|value| !value.is_empty()));
    let mut child =
        Command::new("/home/mirsella/dev/dotfiles/.scratch/pm-resume-20261028/remote/ssh")
            .args(env::args_os().skip(1))
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let status = child.wait()?;
    io::stdout().write_all(&clean(
        out.join().expect("stdout reader panicked")?,
        &secrets,
    ))?;
    io::stderr().write_all(&clean(
        err.join().expect("stderr reader panicked")?,
        &secrets,
    ))?;
    std::process::exit(status.code().unwrap_or(255));
}
