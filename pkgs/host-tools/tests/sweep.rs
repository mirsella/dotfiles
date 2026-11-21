use anyhow::{ensure, Result};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    process::Command,
    time::{Duration, SystemTime},
};

#[test]
fn keeps_files_and_directories_in_place_until_released() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let home = temp.path().canonicalize()?;
    let workspace = home.join("dev/scratch/job");
    let sibling = home.join("dev/scratch/job-other");
    let output = workspace.join("output");
    let file = output.join("report.txt");
    fs::create_dir_all(&output)?;
    fs::create_dir(&sibling)?;
    fs::write(&file, "result\n")?;
    let old = SystemTime::now() - Duration::from_secs(8 * 86_400);
    for path in [&file, &output, &workspace, &sibling] {
        fs::File::open(path)?.set_modified(old)?;
    }

    let sweep = |cwd: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_host-tools"))
            .arg("sweep")
            .args(args)
            .current_dir(cwd)
            .env("HOME", &home)
            .env("STATE_DIRECTORY", home.join("state"))
            .output()
    };
    let run = |cwd: &Path, args: &[&str]| -> Result<String> {
        let result = sweep(cwd, args)?;
        ensure!(
            result.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(String::from_utf8(result.stdout)?)
    };

    let rejected = sweep(&home, &["keep", "dev"])?;
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("not in a workspace managed by sweep")
    );
    let held = run(&workspace, &["keep", "output/report.txt", "--for", "2d"])?;
    assert!(held.contains("Holding workspace dev/scratch/job."));

    let rows: Vec<Value> = serde_json::from_str(&run(&home, &["status", "--json"])?)?;
    let row = rows
        .iter()
        .find(|row| row["path"] == "dev/scratch/job")
        .unwrap();
    assert_eq!(row["state"], "waiting");
    assert!(row["reason"]
        .as_str()
        .unwrap()
        .contains("dev/scratch/job/output/report.txt until"));

    run(&home, &["auto"])?;
    assert_eq!(fs::read_to_string(&file)?, "result\n");
    assert!(!sibling.exists(), "the hold must not match a path prefix");

    let link = workspace.join("link");
    symlink(home.join("missing-target"), &link)?;
    run(&home, &["keep", "dev/scratch/job/link"])?;
    assert!(run(&home, &["keep"])?.contains("dev/scratch/job/link"));
    fs::remove_file(&link)?;
    run(&home, &["unkeep", "dev/scratch/job/link"])?;
    run(&home, &["keep", "dev/scratch/job/output"])?;
    fs::remove_file(&file)?;
    run(&workspace, &["unkeep", "output/report.txt"])?;
    for path in [&output, &workspace] {
        fs::File::open(path)?.set_modified(old)?;
    }
    run(&home, &["clean", "--yes"])?;
    assert!(output.is_dir(), "the directory hold must still apply");

    run(&home, &["unkeep", "dev/scratch/job/output"])?;
    run(&home, &["clean", "--yes"])?;
    assert!(
        !workspace.exists(),
        "the released workspace should be removed"
    );
    Ok(())
}
