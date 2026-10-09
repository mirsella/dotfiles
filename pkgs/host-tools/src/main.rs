mod activity;
mod agent_skills;
mod backup;
mod browser;
mod hardware;
mod hermes;
mod maintenance;
mod memory;
mod monitoring;
mod setup;
mod suspend;
#[cfg(test)]
mod tests;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Recurring host services and maintenance")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    SyncAgentSkills,
    BrowserControl {
        #[arg(long)]
        config: PathBuf,
    },
    HermesProvision {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        ssh_to_age: PathBuf,
        #[arg(long)]
        reuse_mirsellabot: bool,
    },
    HermesConfigure {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        defaults: PathBuf,
    },
    Hermes {
        #[command(subcommand)]
        action: hermes::Operation,
    },
    NightSuspend {
        #[arg(long)]
        activity_only: bool,
    },
    StorageAlerts {
        #[arg(long)]
        test: bool,
    },
    BeszelSetup {
        password_file: PathBuf,
        resend_key_file: PathBuf,
    },
    StorageMail {
        key_file: PathBuf,
        recipient: String,
        subject: String,
    },
    SmartAlert {
        key_file: PathBuf,
        recipient: String,
    },
    Backup {
        config: PathBuf,
    },
    NextcloudSetup {
        config: PathBuf,
    },
    RequireMount {
        dataset: String,
        mountpoint: PathBuf,
        directory: PathBuf,
    },
    LidScreen,
    KeyboardOff,
    Ryzenadj {
        #[arg(long)]
        watch: bool,
    },
    AudioWatch,
    OomProtect {
        #[arg(long)]
        user: String,
    },
    BtrfsSpaceCheck {
        #[arg(long, conflicts_with = "reclaim")]
        notify: bool,
        #[arg(long)]
        reclaim: bool,
        #[arg(default_value = "/")]
        path: PathBuf,
    },
    Wol {
        #[arg(long, default_value = "98:29:a6:3a:a7:50")]
        mac: String,
        #[arg(long, default_value = "192.168.1.255:9")]
        destination: String,
    },
    Netconsole {
        #[arg(long, default_value = "192.168.1.166:16666")]
        bind: String,
        #[arg(long, default_value = "192.168.1.19")]
        sender: std::net::IpAddr,
        #[arg(long)]
        output: PathBuf,
    },
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Commands::SyncAgentSkills => agent_skills::run(),
        Commands::BrowserControl { config } => browser::run(&config),
        Commands::HermesProvision {
            root,
            ssh_to_age,
            reuse_mirsellabot,
        } => hermes::provision(&root, &ssh_to_age, reuse_mirsellabot),
        Commands::Hermes { action } => hermes::operations(action),
        Commands::HermesConfigure { config, defaults } => hermes::configure(&config, &defaults),
        Commands::NightSuspend { activity_only } => suspend::run(activity_only),
        Commands::StorageAlerts { test } => monitoring::alerts(test),
        Commands::BeszelSetup {
            password_file,
            resend_key_file,
        } => setup::beszel(&password_file, &resend_key_file),
        Commands::StorageMail {
            key_file,
            recipient,
            subject,
        } => monitoring::mail(
            &key_file,
            &recipient,
            &subject,
            &std::io::read_to_string(std::io::stdin())?,
        ),
        Commands::SmartAlert {
            key_file,
            recipient,
        } => monitoring::mail(
            &key_file,
            &recipient,
            &std::env::var("SMARTD_SUBJECT")?,
            &std::env::var("SMARTD_FULLMESSAGE")?,
        ),
        Commands::Backup { config } => backup::run(&config),
        Commands::NextcloudSetup { config } => setup::nextcloud(&config),
        Commands::RequireMount {
            dataset,
            mountpoint,
            directory,
        } => {
            util::run(
                "findmnt",
                &[
                    "--source",
                    &dataset,
                    "--mountpoint",
                    util::path(&mountpoint)?,
                ],
            )?;
            std::fs::create_dir_all(directory)?;
            Ok(())
        }
        Commands::LidScreen => hardware::lid(),
        Commands::KeyboardOff => hardware::keyboard_off(),
        Commands::Ryzenadj { watch } => hardware::ryzenadj(watch),
        Commands::AudioWatch => hardware::audio_watch(),
        Commands::OomProtect { user } => memory::run(&user),
        Commands::BtrfsSpaceCheck {
            notify,
            reclaim,
            path,
        } => maintenance::btrfs(&path, notify, reclaim),
        Commands::Wol { mac, destination } => hardware::wol(&mac, &destination),
        Commands::Netconsole {
            bind,
            sender,
            output,
        } => hardware::netconsole(&bind, sender, &output),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("host-tools: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[test]
fn hermes_lifecycle_cli_requires_archive_only_for_restore() {
    assert!(Cli::try_parse_from(["host-tools", "hermes", "restore"]).is_err());
    assert!(Cli::try_parse_from(["host-tools", "hermes", "unknown"]).is_err());
    assert!(
        Cli::try_parse_from([
            "host-tools",
            "hermes",
            "status",
            "--archive",
            "backup.tar.gz"
        ])
        .is_err()
    );
    let parsed = Cli::try_parse_from([
        "host-tools",
        "hermes",
        "restore",
        "--archive",
        "backup.tar.gz",
    ])
    .unwrap();
    assert!(
        matches!(parsed.command, Commands::Hermes { action: hermes::Operation::Restore { archive } } if archive == PathBuf::from("backup.tar.gz"))
    );
}
