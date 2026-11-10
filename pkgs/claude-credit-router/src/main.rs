use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use claude_credit_router::policy::{Mode, Store, private_key, validate_key};
use claude_credit_router::{Config, Router};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "API credits before native subscriptions; never manages subscription limits")]
struct Arguments {
    /// Use the separately configured OpenAI project and native Codex route.
    #[arg(long)]
    openai: bool,
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
    /// Leave this vendor's requests on native subscription authentication.
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
    let directory = if arguments.openai {
        "openai-credit-router"
    } else {
        "claude-credit-router"
    };
    let config_path = arguments.config.unwrap_or(
        home_path("XDG_CONFIG_HOME", ".config")?
            .join(directory)
            .join("config.json"),
    );
    let config: Config =
        serde_json::from_slice(&std::fs::read(&config_path).context("read router configuration")?)
            .context("parse router configuration")?;
    config.validate()?;
    anyhow::ensure!(
        arguments.openai == config.openai.is_some(),
        "CLI vendor must match configuration"
    );
    let store = Store(
        arguments.state.unwrap_or(
            home_path("XDG_STATE_HOME", ".local/state")?
                .join("credit-router")
                .join(directory)
                .join("state.json"),
        ),
    );
    match arguments.command {
        Command::Serve => {
            let variable = if arguments.openai {
                "OPENAI_CREDIT_ROUTER_API_KEY"
            } else {
                "CLAUDE_CREDIT_ROUTER_API_KEY"
            };
            let validate = if arguments.openai {
                claude_credit_router::openai::validate_key
            } else {
                validate_key
            };
            let key = match std::env::var(variable) {
                Ok(value) => validate(&value)?,
                Err(std::env::VarError::NotPresent) => private_key(
                    config
                        .api_key_file
                        .as_deref()
                        .with_context(|| format!("{variable} or api_key_file is required"))?,
                    validate,
                )?,
                Err(_) => anyhow::bail!("{variable} must contain UTF-8 text"),
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
                if arguments.openai
                    && !claude_credit_router::openai::window_open(config.renewal_anchor, Utc::now())
                {
                    status["route"] = serde_json::json!("subscription");
                    status["calendar_guard"] = serde_json::json!(true);
                }
                println!("{}", serde_json::to_string_pretty(&status)?);
                Ok(())
            })?;
        }
    }
    Ok(())
}
