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
    /// Refresh project costs without making an inference request.
    CheckBudget,
    /// Leave this vendor's requests on native subscription authentication.
    Subscription,
    /// Resume automatic promotional/subscription selection.
    Auto,
    /// Try promotional credits on the next model request, not on this command.
    Recheck,
    /// Metadata-only audit history. No model request or credential is needed.
    History {
        #[arg(long)]
        since: Option<chrono::DateTime<Utc>>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, default_value_t = 200)]
        limit: usize,
        /// Group transport and passive OpenCode usage separately; never add them.
        #[arg(long)]
        summary: bool,
        /// Start at the current subscription grant-cycle boundary.
        #[arg(long, conflicts_with = "since")]
        grant_cycle: bool,
        /// Optional read-only T3 database for exact native-session bindings.
        #[arg(long)]
        t3_db: Option<PathBuf>,
    },
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
        Command::History {
            since,
            session,
            limit,
            summary,
            grant_cycle,
            t3_db,
        } => {
            let since = if grant_cycle {
                Some(claude_credit_router::policy::cycle_start(
                    config.renewal_anchor,
                    Utc::now(),
                )?)
            } else {
                since
            };
            store.read()?;
            let mut records = store.audit().records(None)?;
            claude_credit_router::audit::correlate(&mut records);
            let path =
                t3_db.unwrap_or(home_path("T3_HOME", ".t3")?.join("userdata/statev2.sqlite"));
            let bindings = if path.exists() {
                claude_credit_router::audit::thread_bindings(&path).unwrap_or_else(|error| {
                    eprintln!("credit-router: T3 binding lookup unavailable: {error}");
                    Default::default()
                })
            } else {
                Default::default()
            };
            let output = if summary {
                claude_credit_router::audit::report_filtered(
                    &records,
                    &bindings,
                    since,
                    session.as_deref(),
                )
            } else {
                records.retain(|record| {
                    since.is_none_or(|since| record.at >= since)
                        && session.as_ref().is_none_or(|session| {
                            record.context.session_id.as_ref() == Some(session)
                        })
                });
                serde_json::to_value(&records[records.len().saturating_sub(limit)..])?
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Command::CheckBudget => {
            let allowed = claude_credit_router::spending::Guard::new()?
                .check(&config, &store, &Utc::now, true)
                .await?;
            println!(
                "{}",
                serde_json::json!({"budget_allows_api":allowed,"state":store.read()?})
            );
        }
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
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .context("install SIGTERM handler")?;
            eprintln!(
                "claude-credit-router: listening on {} for organization {}",
                listener.local_addr()?,
                router.config.organization_id
            );
            axum::serve(listener, router.app())
                .with_graceful_shutdown(async move {
                    tokio::select! {
                        result = tokio::signal::ctrl_c() => {
                            if let Err(error) = result {
                                eprintln!("credit-router: SIGINT handler failed: {error}; shutting down");
                            }
                        }
                        _ = terminate.recv() => {}
                    }
                    eprintln!("credit-router: draining active requests");
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
                    Command::CheckBudget => unreachable!("budget check handled above"),
                    Command::Serve => unreachable!("serve was handled above"),
                    Command::History { .. } => unreachable!("history was handled above"),
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
