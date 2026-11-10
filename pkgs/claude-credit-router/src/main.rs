use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use claude_credit_router::policy::{Mode, Store, private_key, validate_key};
use claude_credit_router::{Config, Router};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Promotional-only Claude API routing; never manages subscription limits")]
struct Arguments {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    state: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve,
    Status,
    /// Leave all Claude requests on native subscription authentication.
    Subscription,
    /// Resume automatic promotional/subscription selection.
    Auto,
    /// Try promotional credits on the next model request, not on this command.
    Recheck,
}

fn home_path(variable: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(value) = std::env::var_os(variable) {
        return Ok(value.into());
    }
    Ok(PathBuf::from(std::env::var_os("HOME").context("HOME is required")?).join(fallback))
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();
    let config_path = arguments.config.unwrap_or(
        home_path("XDG_CONFIG_HOME", ".config")?.join("claude-credit-router/config.json"),
    );
    let config: Config =
        serde_json::from_slice(&std::fs::read(&config_path).context("read router configuration")?)
            .context("parse router configuration")?;
    config.validate()?;
    let store = Store(arguments.state.unwrap_or(
        home_path("XDG_STATE_HOME", ".local/state")?.join("claude-credit-router/state.json"),
    ));
    match arguments.command {
        Command::Serve => {
            let key = match std::env::var("CLAUDE_CREDIT_ROUTER_API_KEY") {
                Ok(value) => validate_key(&value)?,
                Err(std::env::VarError::NotPresent) => private_key(
                    config
                        .api_key_file
                        .as_deref()
                        .context("CLAUDE_CREDIT_ROUTER_API_KEY or api_key_file is required")?,
                )?,
                Err(_) => anyhow::bail!("CLAUDE_CREDIT_ROUTER_API_KEY must contain UTF-8 text"),
            };
            let router = Router::new(config, store, &key)?;
            router.verify_organization().await?;
            let listener = tokio::net::TcpListener::bind(router.config.listen).await?;
            eprintln!(
                "claude-credit-router: listening on {} for organization {}",
                listener.local_addr()?,
                router.config.organization_id
            );
            axum::serve(listener, router.app())
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
        command => {
            store.update(|state| {
                match command {
                    Command::Subscription => state.set_mode(Mode::Subscription, Utc::now()),
                    Command::Auto => state.set_mode(Mode::Auto, Utc::now()),
                    Command::Recheck => state.recheck(Utc::now()),
                    Command::Status => {}
                    Command::Serve => unreachable!("serve was handled above"),
                }
                let mut status = serde_json::to_value(&*state)?;
                status["route"] = serde_json::to_value(state.route(Utc::now()))?;
                println!("{}", serde_json::to_string_pretty(&status)?);
                Ok(())
            })?;
        }
    }
    Ok(())
}
