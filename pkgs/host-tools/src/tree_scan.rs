//! A parallel walk of one workspace, in the manner of fast file finders: many
//! threads share a queue of directories and read each one once. It stays on
//! one filesystem and never follows a symlink.
//!
//! By default only directories are stat'ed: the listing already gives every
//! entry's type, and a directory's time moves whenever something inside it is
//! created, renamed or deleted, which is how builds, editors and agents write.
//! Nested `.git` directories are not walked; their HEAD, index and reflog
//! times stand for them. Sizes need every file, so only `sweep`'s listing asks
//! for them.
//!
//! While a run remembers, the threads it hands work to share one scan per
//! workspace and one look at running processes.
use anyhow::{Result, anyhow};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::ErrorKind,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::SystemTime,
};

/// What a walk found below a workspace.
#[derive(Debug)]
pub(crate) struct Scan {
    /// Disk usage on the workspace's filesystem, when sizes were asked for.
    pub(crate) size: Option<u64>,
    /// The newest change, and where; the workspace itself counts.
    pub(crate) newest: SystemTime,
    pub(crate) newest_path: PathBuf,
    /// Directories holding a `.git` of their own: nested checkouts.
    pub(crate) checkouts: Vec<PathBuf>,
    /// Other filesystems and bare repositories.
    pub(crate) problems: Vec<String>,
    /// What another user's permissions hide, such as a container's data.
    pub(crate) unreadable: Vec<PathBuf>,
    /// Something disappeared mid-walk, so the workspace is in use.
    pub(crate) vanished: bool,
}

#[derive(Default)]
struct Found {
    size: u64,
    newest: Option<(SystemTime, PathBuf)>,
    checkouts: Vec<PathBuf>,
    problems: Vec<String>,
    unreadable: Vec<PathBuf>,
    vanished: bool,
    error: Option<anyhow::Error>,
}

impl Found {
    fn saw(&mut self, time: SystemTime, path: impl FnOnce() -> PathBuf) {
        if self
            .newest
            .as_ref()
            .is_none_or(|(newest, _)| time > *newest)
        {
            self.newest = Some((time, path()));
        }
    }

    fn merge(&mut self, other: Found) {
        self.size += other.size;
        if let Some((time, path)) = other.newest {
            self.saw(time, || path);
        }
        self.checkouts.extend(other.checkouts);
        self.problems.extend(other.problems);
        self.unreadable.extend(other.unreadable);
        self.vanished |= other.vanished;
        if self.error.is_none() {
            self.error = other.error;
        }
    }
}

/// Package-manager caches: the Git repositories they hold are downloads that
/// the next build fetches again, not work.
const CACHES: &[&[u8]] = &[
    b".build",
    b"node_modules",
    b"target",
    b".cargo",
    b".gradle",
    b".pnpm-store",
];

/// A directory waiting to be read, and whether it lies inside a `.git` or a
/// package cache, where Git checkouts are not work.
type Job = (PathBuf, bool);

struct Walk<'a> {
    root: &'a Path,
    device: u64,
    own_git: Option<&'a Path>,
    stop_at: Option<SystemTime>,
    sizes: bool,
    /// Work shared with idle workers; each worker mostly runs its own stack.
    shared: Mutex<Vec<Job>>,
    ready: Condvar,
    idle: AtomicUsize,
    /// Directories found and not yet read, in any stack: zero means done.
    pending: AtomicUsize,
    stop: AtomicBool,
}

impl Walk<'_> {
    /// Wait for shared work, or for the walk to end.
    fn take(&self) -> Option<Job> {
        let mut shared = self.shared.lock().expect("scan queue poisoned");
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return None;
            }
            if let Some(job) = shared.pop() {
                return Some(job);
            }
            if self.pending.load(Ordering::Acquire) == 0 {
                return None;
            }
            self.idle.fetch_add(1, Ordering::AcqRel);
            shared = self.ready.wait(shared).expect("scan queue poisoned");
            self.idle.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// Wake everyone, under the lock so no waiter misses it.
    fn wake_all(&self) {
        let _shared = self.shared.lock().expect("scan queue poisoned");
        self.ready.notify_all();
    }

    fn halt(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake_all();
    }

    fn work(&self) -> Found {
        let mut found = Found::default();
        let mut stack = Vec::new();
        while let Some((directory, inside_git)) = stack.pop().or_else(|| self.take()) {
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            let children = self.read(&directory, inside_git, &mut found);
            self.pending.fetch_add(children.len(), Ordering::AcqRel);
            stack.extend(children);
            if found.error.is_some()
                || self
                    .stop_at
                    .zip(found.newest.as_ref())
                    .is_some_and(|(stop, (newest, _))| *newest >= stop)
            {
                self.halt();
                break;
            }
            // Share half the stack, oldest first, only when someone is idle.
            let idle = self.idle.load(Ordering::Acquire);
            if idle > 0 && stack.len() > 1 {
                let give = (stack.len() / 2).max(1);
                let mut shared = self.shared.lock().expect("scan queue poisoned");
                shared.extend(stack.drain(..give));
                drop(shared);
                if give >= idle {
                    self.ready.notify_all();
                } else {
                    for _ in 0..give {
                        self.ready.notify_one();
                    }
                }
            }
            if self.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
                self.wake_all();
            }
        }
        found
    }

    /// A nested repository's own activity, without walking its objects.
    fn note_repository(&self, git_dir: &Path, found: &mut Found) {
        for name in ["HEAD", "index", "logs/HEAD"] {
            let path = git_dir.join(name);
            if let Ok(time) = path.symlink_metadata().and_then(|info| info.modified()) {
                found.saw(time, || path);
            }
        }
    }

    /// Read one directory; returns its subdirectories to walk.
    fn read(&self, directory: &Path, inside_git: bool, found: &mut Found) -> Vec<Job> {
        let mut subdirectories = Vec::new();
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            // Files another user owns, such as a container's data.
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                found.unreadable.push(directory.to_owned());
                return subdirectories;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                found.vanished = true;
                return subdirectories;
            }
            Err(error) => {
                found.error = Some(anyhow!(error).context(format!("read {}", directory.display())));
                return subdirectories;
            }
        };
        // A directory holding HEAD, objects and refs is a bare repository.
        let mut bare = [false; 3];
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    found.error =
                        Some(anyhow!(error).context(format!("read {}", directory.display())));
                    return subdirectories;
                }
            };
            // The type comes with the directory listing, so a symlink costs
            // nothing: its own time says nothing about use and it is not followed.
            let Ok(kind) = entry.file_type() else {
                found.vanished = true;
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let name = entry.file_name();
            if !inside_git {
                match name.to_str() {
                    Some("HEAD") if kind.is_file() => bare[0] = true,
                    Some("objects") if kind.is_dir() => bare[1] = true,
                    Some("refs") if kind.is_dir() => bare[2] = true,
                    _ => {}
                }
            }
            let is_git = name == OsStr::new(".git");
            if is_git {
                let path = entry.path();
                if self.own_git == Some(path.as_path()) {
                    if !kind.is_file() {
                        found
                            .problems
                            .push("its .git is not a linked worktree's".into());
                    }
                } else if !inside_git {
                    found.checkouts.push(directory.to_owned());
                }
            }
            // Without sizes, a file's own time is not needed: the directory's
            // time covers every file created, renamed or deleted in it.
            if !kind.is_dir() && !self.sizes {
                continue;
            }
            // Stat relative to the open directory, not by full path.
            let info = match entry.metadata() {
                Ok(info) => info,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    found.vanished = true;
                    continue;
                }
                Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                    found.unreadable.push(entry.path());
                    continue;
                }
                Err(error) => {
                    found.error =
                        Some(anyhow!(error).context(format!("stat {}", entry.path().display())));
                    return subdirectories;
                }
            };
            if info.dev() != self.device {
                found.problems.push(format!(
                    "{} is on another filesystem",
                    entry.path().display()
                ));
                continue;
            }
            found.size += info.blocks() * 512;
            if let Ok(time) = info.modified() {
                found.saw(time, || entry.path());
            }
            if kind.is_dir() {
                if is_git && !self.sizes {
                    self.note_repository(&entry.path(), found);
                } else {
                    let quiet = inside_git || is_git || CACHES.contains(&name.as_encoded_bytes());
                    subdirectories.push((entry.path(), quiet));
                }
            }
        }
        if bare == [true; 3] && directory != self.root {
            found
                .problems
                .push(format!("{} is a bare Git repository", directory.display()));
        }
        subdirectories
    }
}

/// Scans running now; concurrent scans split the cores between them.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

struct Active;

impl Active {
    fn enter() -> Self {
        ACTIVE.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

fn cores() -> usize {
    std::thread::available_parallelism().map_or(8, |count| count.get().min(64))
}

fn workers() -> usize {
    // Metadata lookups wait on the kernel and the disk, not on CPU: one per core.
    (cores() / ACTIVE.load(Ordering::Acquire).max(1)).max(2)
}

/// Walk `root`, which lies on `device`. A linked worktree passes its own
/// `.git` file as `own_git`. With `stop_at`, the walk ends at the first change
/// that recent, and the result says only that.
pub(crate) fn scan(
    root: &Path,
    device: u64,
    own_git: Option<&Path>,
    stop_at: Option<SystemTime>,
    sizes: bool,
) -> Result<Scan> {
    let _active = Active::enter();
    let info = root.symlink_metadata()?;
    let walk = Walk {
        root,
        device,
        own_git,
        stop_at,
        sizes,
        shared: Mutex::new(vec![(root.to_owned(), false)]),
        ready: Condvar::new(),
        idle: AtomicUsize::new(0),
        pending: AtomicUsize::new(1),
        stop: AtomicBool::new(false),
    };
    let mut found = Found::default();
    found.saw(info.modified()?, || root.to_owned());
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..workers())
            .map(|_| scope.spawn(|| walk.work()))
            .collect();
        for thread in threads {
            found.merge(thread.join().expect("scan worker panicked"));
        }
    });
    if let Some(error) = found.error {
        return Err(error);
    }
    let (newest, newest_path) = found.newest.expect("the root was recorded");
    found.checkouts.sort();
    found.problems.sort();
    Ok(Scan {
        size: sizes.then_some(found.size),
        newest,
        newest_path,
        checkouts: found.checkouts,
        problems: found.problems,
        unreadable: found.unreadable,
        vanished: found.vanished,
    })
}

/// What one remembering run shares between its threads.
#[derive(Default)]
struct Memory {
    scans: Mutex<HashMap<PathBuf, Arc<Scan>>>,
    processes: Mutex<Option<Arc<Vec<PathBuf>>>>,
}

thread_local! {
    static MEMORY: RefCell<Option<Arc<Memory>>> = const { RefCell::new(None) };
}

fn memory() -> Option<Arc<Memory>> {
    MEMORY.with_borrow(Clone::clone)
}

/// Keep complete scans and the process list until the guard drops. Only a
/// read-only pass may do this: removal must judge fresh state.
/// Inside a remembering run, the run's memory stays in use.
pub(crate) fn remember() -> Remembering {
    if remembering() {
        return Remembering(false);
    }
    MEMORY.set(Some(Arc::default()));
    Remembering(true)
}

pub(crate) struct Remembering(bool);

impl Drop for Remembering {
    fn drop(&mut self) {
        if self.0 {
            MEMORY.set(None);
        }
    }
}

pub(crate) fn remembering() -> bool {
    MEMORY.with_borrow(Option::is_some)
}

/// The complete scan of `root` this run already holds, if any.
pub(crate) fn remembered(root: &Path) -> Option<Arc<Scan>> {
    let memory = memory()?;
    let scans = memory.scans.lock().expect("scan memory poisoned");
    scans.get(root).cloned()
}

/// A complete scan of `root`, walked once per run while remembering. A scan
/// with sizes also answers a request without them.
pub(crate) fn complete(
    root: &Path,
    device: u64,
    own_git: Option<&Path>,
    sizes: bool,
) -> Result<Arc<Scan>> {
    if let Some(scan) = remembered(root).filter(|scan| scan.size.is_some() || !sizes) {
        return Ok(scan);
    }
    let scan = Arc::new(scan(root, device, own_git, None, sizes)?);
    if let Some(memory) = memory() {
        let mut scans = memory.scans.lock().expect("scan memory poisoned");
        scans.insert(root.to_owned(), scan.clone());
    }
    Ok(scan)
}

/// Paths running processes use, read once per run while remembering.
pub(crate) fn processes(load: impl FnOnce() -> Result<Vec<PathBuf>>) -> Result<Arc<Vec<PathBuf>>> {
    let Some(memory) = memory() else {
        return Ok(Arc::new(load()?));
    };
    let mut cached = memory.processes.lock().expect("process memory poisoned");
    if let Some(paths) = cached.as_ref() {
        return Ok(paths.clone());
    }
    let paths = Arc::new(load()?);
    *cached = Some(paths.clone());
    Ok(paths)
}

/// Map `items` on several threads, in order. Each thread shares this run's
/// memory, and the scans inside split the cores between them.
pub(crate) fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = items.len().min(16);
    if threads <= 1 {
        return items.iter().map(f).collect();
    }
    let shared = memory();
    let next = AtomicUsize::new(0);
    let mut results: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    MEMORY.set(shared.clone());
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break;
                        };
                        done.push((index, f(item)));
                    }
                    MEMORY.set(None);
                    done
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("parallel worker panicked"))
            .collect()
    });
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn age(path: &Path, time: SystemTime) -> Result<()> {
        fs::File::open(path)?.set_modified(time)?;
        Ok(())
    }

    #[test]
    fn scans_sizes_times_checkouts_and_problems_without_following_symlinks() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("workspace");
        let old = SystemTime::now() - Duration::from_secs(10 * 86_400);
        // Package caches hold Git repositories that builds fetch again.
        let caches = [
            "node_modules/dep/.git",
            ".build/repositories/swift/objects",
            ".build/repositories/swift/refs",
        ];
        for dir in ["a/b/c", "clone/.git/objects", "bare/objects", "bare/refs"]
            .into_iter()
            .chain(caches)
        {
            fs::create_dir_all(root.join(dir))?;
        }
        fs::write(root.join("a/b/c/data"), vec![7u8; 64 * 1024])?;
        fs::write(root.join("clone/.git/HEAD"), "ref: refs/heads/main\n")?;
        fs::write(root.join("bare/HEAD"), "ref: refs/heads/main\n")?;
        fs::write(
            root.join("node_modules/dep/.git/HEAD"),
            "ref: refs/heads/main\n",
        )?;
        fs::write(
            root.join(".build/repositories/swift/HEAD"),
            "ref: refs/heads/main\n",
        )?;
        fs::write(root.join(".git"), "gitdir: elsewhere\n")?;
        let outside = temp.path().join("outside");
        fs::create_dir(&outside)?;
        fs::write(outside.join("big"), vec![1u8; 1 << 20])?;
        std::os::unix::fs::symlink(&outside, root.join("link"))?;
        let file = root.join("a/b/c/data");
        let directory = root.join("a/b");
        let mut all = vec![root.clone()];
        for entry in [
            "a",
            "a/b",
            "a/b/c",
            "a/b/c/data",
            "clone",
            "clone/.git",
            "clone/.git/objects",
            "clone/.git/HEAD",
            "bare",
            "bare/objects",
            "bare/refs",
            "bare/HEAD",
            ".git",
            "node_modules",
            "node_modules/dep",
            "node_modules/dep/.git",
            "node_modules/dep/.git/HEAD",
            ".build",
            ".build/repositories",
            ".build/repositories/swift",
            ".build/repositories/swift/objects",
            ".build/repositories/swift/refs",
            ".build/repositories/swift/HEAD",
        ] {
            all.push(root.join(entry));
        }
        for path in &all {
            age(path, old)?;
        }
        age(&directory, old + Duration::from_secs(30))?;
        age(&file, old + Duration::from_secs(60))?;
        let device = root.metadata()?.dev();
        let own = root.join(".git");

        let full = scan(&root, device, Some(&own), None, true)?;
        let size = full.size.expect("sizes were asked for");
        assert!(size >= 64 * 1024 && size < 1 << 20, "{size}");
        assert_eq!(full.newest_path, file);
        assert_eq!(full.checkouts, vec![root.join("clone")]);
        assert_eq!(full.problems.len(), 1, "{:?}", full.problems);
        assert!(full.problems[0].contains("bare Git repository"));
        assert!(!full.vanished);

        // Without sizes only directories, and a nested repository's HEAD, count.
        let names = scan(&root, device, Some(&own), None, false)?;
        assert_eq!(names.size, None);
        assert_eq!(names.newest_path, directory);
        assert_eq!(names.checkouts, full.checkouts);
        assert_eq!(names.problems, full.problems);
        age(&root.join("clone/.git/HEAD"), old + Duration::from_secs(90))?;
        let names = scan(&root, device, Some(&own), None, false)?;
        assert_eq!(names.newest_path, root.join("clone/.git/HEAD"));

        // Stopping early still names a change at least that recent.
        let stopped = scan(
            &root,
            device,
            Some(&own),
            Some(old + Duration::from_secs(1)),
            true,
        )?;
        assert!(stopped.newest > old);
        Ok(())
    }

    #[test]
    fn scans_are_shared_only_while_remembering() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("workspace");
        fs::create_dir(&root)?;
        let device = root.metadata()?.dev();
        let first = complete(&root, device, None, false)?;
        assert!(!Arc::ptr_eq(&first, &complete(&root, device, None, false)?));
        let guard = remember();
        let names = complete(&root, device, None, false)?;
        // Other threads of the run see the same scan.
        let seen = parallel(&[0, 1, 2], |_| remembered(&root).is_some());
        assert_eq!(seen, vec![true; 3]);
        assert!(Arc::ptr_eq(&names, &complete(&root, device, None, false)?));
        // Asking for sizes walks again, and that scan answers both.
        let sized = complete(&root, device, None, true)?;
        assert!(!Arc::ptr_eq(&names, &sized));
        assert!(Arc::ptr_eq(&sized, &complete(&root, device, None, false)?));
        drop(guard);
        assert!(remembered(&root).is_none());
        Ok(())
    }

    #[test]
    fn parallel_keeps_order() {
        let items: Vec<u32> = (0..100).collect();
        assert_eq!(
            parallel(&items, |n| n * 2),
            (0..100).map(|n| n * 2).collect::<Vec<_>>()
        );
    }
}
