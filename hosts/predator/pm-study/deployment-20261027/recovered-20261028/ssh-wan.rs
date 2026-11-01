use std::{env, ffi::OsString, os::unix::process::CommandExt, process::Command};

fn main() {
    let input: Vec<OsString> = env::args_os().skip(1).collect();
    let hosts: Vec<_> = input
        .iter()
        .filter_map(|arg| match arg.to_str() {
            Some("192.168.1.19") => Some("22"),
            Some("192.168.1.166") => Some("222"),
            _ => None,
        })
        .collect();
    assert_eq!(hosts.len(), 1, "expected exactly one study SSH destination");
    let mut args = Vec::new();
    let mut iter = input.iter();
    while let Some(arg) = iter.next() {
        if arg == "-p" {
            assert!(iter.next().is_some(), "missing SSH port value");
        } else {
            assert!(
                !arg.to_string_lossy().starts_with("-p"),
                "use a separate -p argument"
            );
            args.push(arg);
        }
    }
    let error = Command::new("/usr/bin/ssh")
        .args([
            "-o",
            "HostName=mirsella.mooo.com",
            "-o",
            "StrictHostKeyChecking=yes",
            "-p",
            hosts[0],
        ])
        .args(args)
        .exec();
    panic!("cannot execute SSH transport: {error}");
}
