use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const ROOTS: [(&str, &str); 3] = [
    ("/srv/storage", "tank"),
    ("/srv/backup", "backup"),
    ("/var/lib/nextcloud/data", "fast"),
];

const INTEREST: WatchMask = WatchMask::CREATE
    .union(WatchMask::DELETE)
    .union(WatchMask::MODIFY)
    .union(WatchMask::MOVED_FROM)
    .union(WatchMask::MOVED_TO);

fn log(msg: &str) {
    println!("{msg}");
}

fn touch(flagdir: &Path, flag: &str) {
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(flagdir.join(format!("{flag}.dirty")));
}

fn dirty_all(flagdir: &Path) {
    for flag in ROOTS.iter().map(|(_, f)| f).collect::<HashSet<_>>() {
        touch(flagdir, flag);
    }
}

fn add_tree(
    inotify: &mut Inotify,
    watched: &mut HashMap<WatchDescriptor, (PathBuf, String)>,
    dir: &Path,
    flag: &str,
) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        match inotify.watches().add(&d, INTEREST) {
            Ok(wd) => {
                watched.insert(wd, (d.clone(), flag.to_string()));
            }
            Err(_) => continue,
        }
        let entries = match fs::read_dir(&d) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(p);
            }
        }
    }
}

fn run(flagdir: &Path) -> ExitCode {
    if let Err(e) = fs::create_dir_all(flagdir) {
        eprintln!("cannot create {flagdir:?}: {e}");
        return ExitCode::FAILURE;
    }
    let mut inotify = match Inotify::init() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("inotify_init failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut watched: HashMap<WatchDescriptor, (PathBuf, String)> = HashMap::new();
    for (root, flag) in ROOTS {
        let path = Path::new(root);
        if !path.is_dir() {
            log(&format!("root missing, skipping: {root}"));
            continue;
        }
        add_tree(&mut inotify, &mut watched, path, flag);
    }
    log(&format!("watching {} dirs", watched.len()));

    dirty_all(flagdir);
    log("startup: all flags marked dirty");

    let mut buffer = [0u8; 256 * 1024];
    loop {
        let events = match inotify.read_events_blocking(&mut buffer) {
            Ok(e) => e,
            Err(e) => {
                log(&format!("error, marking everything dirty and exiting: {e}"));
                dirty_all(flagdir);
                return ExitCode::FAILURE;
            }
        };
        for event in events {
            if event.mask.contains(EventMask::Q_OVERFLOW) {
                log("queue overflow, marking everything dirty");
                dirty_all(flagdir);
                continue;
            }
            if event.mask.contains(EventMask::IGNORED) {
                watched.remove(&event.wd);
                continue;
            }
            let Some((path, flag)) = watched.get(&event.wd).cloned() else {
                continue;
            };
            if event.mask.intersects(
                EventMask::MODIFY
                    | EventMask::CREATE
                    | EventMask::DELETE
                    | EventMask::MOVED_FROM
                    | EventMask::MOVED_TO,
            ) {
                touch(flagdir, &flag);
            }
            if event.mask.contains(EventMask::CREATE) && event.mask.contains(EventMask::ISDIR) {
                if let Some(name) = event.name {
                    let full = path.join(name);
                    if full.is_dir() {
                        add_tree(&mut inotify, &mut watched, &full, &flag);
                    }
                }
            }
        }
    }
}

fn main() -> ExitCode {
    let flagdir = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: zfs-dirty-flag <flagdir>");
        std::process::exit(2);
    });
    run(Path::new(&flagdir))
}
