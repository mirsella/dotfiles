use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
use ureq::{Agent, http::HeaderValue};
use url::{Origin, Url};

#[derive(Args)]
pub struct Options {
    /// Exact HTTPS page origin, such as https://login.example.com.
    #[arg(long)]
    origin: String,
    /// Choose a specific tab when several pages share the origin.
    #[arg(long)]
    tab: Option<String>,
    /// Input reference from the latest native browser snapshot.
    #[arg(long = "ref")]
    reference: String,
    /// Extract a string from JSON stdout, for example /totp.
    #[arg(long)]
    json_pointer: Option<String>,
    /// Secret-producing command; its arguments must not contain secret values.
    #[arg(last = true, required = true, num_args = 1..)]
    command: Vec<String>,
}

struct Browser {
    endpoint: Url,
    authorization: HeaderValue,
    user: String,
    client: Agent,
}

#[derive(Deserialize)]
struct Tabs {
    tabs: Vec<Tab>,
}
#[derive(Deserialize)]
struct Tab {
    /// A tab whose entry is missing or whose value is not a string cannot be
    /// addressed at all, so it stays unselectable instead of aborting the
    /// listing: one bad entry cannot hide the other tabs.
    #[serde(rename = "tabId")]
    id: Option<Value>,
    /// Missing while the native browser no longer reports the tracked tab. The
    /// URL stays unparsed, and any value that is not a string counts as
    /// unreadable, so one bad entry cannot hide the other tabs.
    url: Option<Value>,
}

impl Tab {
    fn id(&self) -> Option<&str> {
        self.id.as_ref().and_then(Value::as_str)
    }
}

fn configuration() -> Result<Browser> {
    let names = ["CAMOFOX_URL", "CAMOFOX_API_KEY", "CAMOFOX_USER_ID"];
    let mut values = std::collections::BTreeMap::new();
    for name in names {
        match env::var(name) {
            Ok(value) => {
                values.insert(name.to_owned(), value);
            }
            Err(env::VarError::NotPresent) => {}
            Err(env::VarError::NotUnicode(_)) => bail!("{name} must be UTF-8"),
        }
    }
    if values.len() != names.len() {
        let home = PathBuf::from(
            env::var_os("HERMES_HOME")
                .context("Run inside Hermes, or set HERMES_HOME to its state directory")?,
        );
        for (name, value) in
            crate::hermes::parse_environment(&fs::read_to_string(home.join(".env"))?)?
        {
            if names.contains(&name.as_str()) {
                values.entry(name).or_insert(value);
            }
        }
    }
    let mut required = |name: &str| {
        values
            .remove(name)
            .filter(|value| !value.is_empty())
            .with_context(|| format!("Missing {name} in Hermes's environment"))
    };
    let endpoint: Url = required("CAMOFOX_URL")?
        .parse()
        .context("Invalid Camofox endpoint")?;
    ensure!(
        crate::browser::private_url(&endpoint, "http")
            && endpoint.path() == "/"
            && endpoint.query().is_none()
            && endpoint.fragment().is_none(),
        "Credential bridge requires a literal loopback HTTP endpoint without URL credentials"
    );
    let mut authorization =
        HeaderValue::from_str(&format!("Bearer {}", required("CAMOFOX_API_KEY")?))
            .context("Invalid Camofox authorization header")?;
    authorization.set_sensitive(true);
    Ok(Browser {
        endpoint,
        authorization,
        user: required("CAMOFOX_USER_ID")?,
        client: local_client(),
    })
}

fn local_client() -> Agent {
    Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(90)))
        .build()
        .into()
}

fn select_tab<'a>(tabs: &'a [Tab], origin: &Origin, id: Option<&str>) -> Result<&'a Tab> {
    let mut matching = tabs.iter().filter(|tab| {
        let Some(current) = tab.id() else {
            return false;
        };
        id.is_none_or(|id| id == current)
            && tab
                .url
                .as_ref()
                .and_then(Value::as_str)
                .and_then(|url| url.parse::<Url>().ok())
                .is_some_and(|url| url.origin() == *origin)
    });
    let first = matching
        .next()
        .context("No matching browser page; check its HTTPS origin and tab")?;
    if let Some(second) = matching.next() {
        bail!(
            "Several pages match; use --tab with one of: {}",
            [first, second]
                .into_iter()
                .chain(matching)
                .filter_map(Tab::id)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(first)
}

fn secret(options: &Options) -> Result<String> {
    let [program, args @ ..] = options.command.as_slice() else {
        unreachable!("clap requires at least one secret command argument")
    };
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("Cannot run secret source command")?;
    ensure!(
        output.status.success(),
        "Secret source failed ({})",
        output.status
    );
    let value = if let Some(pointer) = &options.json_pointer {
        let mut value: Value =
            serde_json::from_slice(&output.stdout).context("Secret source did not return JSON")?;
        match value.pointer_mut(pointer).map(Value::take) {
            Some(Value::String(value)) => value,
            _ => bail!("JSON pointer must select a secret string"),
        }
    } else {
        let mut text = String::from_utf8(output.stdout).context("Secret source is not UTF-8")?;
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        text
    };
    ensure!(
        !value.is_empty() && value.len() <= 32_768,
        "Secret source must return 1-32768 bytes"
    );
    Ok(value)
}

fn fill(browser: &Browser, options: &Options) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Fill<'a> {
        user_id: &'a str,
        #[serde(rename = "ref")]
        reference: &'a str,
        text: &'a str,
        expected_origin: &'a str,
    }
    #[derive(Deserialize)]
    struct Confirmation {
        ok: bool,
    }

    let origin = crate::browser::https_origin(&options.origin, "page origin")?.origin();
    let reference = options
        .reference
        .strip_prefix('@')
        .unwrap_or(&options.reference);
    ensure!(
        reference.strip_prefix('e').is_some_and(
            |number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        ),
        "Use an input ref from the latest snapshot, such as e12"
    );
    let mut response = browser
        .client
        .get(browser.endpoint.join("tabs")?.as_str())
        .header("Authorization", browser.authorization.clone())
        .query_pairs([("userId", browser.user.as_str())])
        .call()?;
    ensure!(
        response.status().is_success(),
        "Cannot list browser tabs (HTTP {})",
        response.status()
    );
    let tabs: Tabs = response
        .body_mut()
        .read_json()
        .map_err(|_| anyhow!("Invalid native browser tab list"))?;
    let tab = select_tab(&tabs.tabs, &origin, options.tab.as_deref())?;
    let value = secret(options)?;
    let tab_id = tab
        .id()
        .context("Selected browser tab lost its identifier")?;
    let mut target = browser.endpoint.clone();
    target
        .path_segments_mut()
        .expect("validated HTTP URL")
        .extend(["tabs", tab_id, "type"]);
    let mut response = browser
        .client
        .post(target.as_str())
        .header("Authorization", browser.authorization.clone())
        .send_json(&Fill {
            user_id: &browser.user,
            reference,
            text: &value,
            expected_origin: &options.origin,
        })?;
    ensure!(
        response.status().is_success(),
        "Browser refused secret fill (HTTP {}); refresh the page and snapshot",
        response.status()
    );
    let result: Confirmation = response
        .body_mut()
        .read_json()
        .map_err(|_| anyhow!("Invalid native browser fill response"))?;
    ensure!(result.ok, "Native browser did not confirm the fill");
    Ok(())
}

pub fn run(options: Options) -> Result<()> {
    fill(&configuration()?, &options)?;
    println!("Filled browser input; secret value withheld.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::State,
        http::StatusCode,
        routing::{get, post},
    };
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn browser(endpoint: Url) -> Browser {
        let mut authorization = HeaderValue::from_static("Bearer private-key");
        authorization.set_sensitive(true);
        Browser {
            endpoint,
            authorization,
            user: "home-browser".into(),
            client: local_client(),
        }
    }

    fn options() -> Options {
        Options {
            origin: "https://login.example.com".into(),
            tab: None,
            reference: "@e12".into(),
            json_pointer: None,
            command: vec![
                "sh".into(),
                "-c".into(),
                "printf '  dummy-password  \\n'".into(),
            ],
        }
    }

    #[test]
    fn selection_and_secret_decoding_preserve_values_without_guessing() {
        let tabs = vec![
            Tab {
                id: Some("a".into()),
                url: Some("https://login.example.com/form".into()),
            },
            Tab {
                id: Some("b".into()),
                url: Some("https://login.example.com/other".into()),
            },
            Tab {
                id: Some("unreported".into()),
                url: None,
            },
            Tab {
                id: Some("unreadable".into()),
                url: Some("not a URL".into()),
            },
            Tab {
                id: Some(json!(7)),
                url: Some("https://login.example.com/numbered".into()),
            },
        ];
        let origin = "https://login.example.com/form"
            .parse::<Url>()
            .unwrap()
            .origin();
        let other: Url = "https://other.example.com".parse().unwrap();
        assert!(select_tab(&tabs, &other.origin(), None).is_err());
        assert!(select_tab(&tabs, &origin, None).is_err());
        assert_eq!(
            select_tab(&tabs, &origin, Some("b")).unwrap().id(),
            Some("b")
        );
        assert_eq!(
            select_tab(&tabs[..1], &origin, None).unwrap().id(),
            Some("a")
        );
        assert!(select_tab(&tabs, &origin, Some("missing")).is_err());
        // A tab the native browser no longer reports is never matched.
        assert!(select_tab(&tabs, &origin, Some("unreported")).is_err());
        // One unreadable URL must not hide the tabs the bridge can still use.
        assert!(select_tab(&tabs, &origin, Some("unreadable")).is_err());
        // Neither must an identifier that is not a string: it is unaddressable,
        // so it stays out of the match set and out of the disambiguation list.
        assert!(select_tab(&tabs, &origin, Some("7")).is_err());
        assert!(
            select_tab(&tabs, &origin, None)
                .err()
                .unwrap()
                .to_string()
                .ends_with("a, b")
        );
        assert_eq!(secret(&options()).unwrap(), "  dummy-password  ");
        let mut options = options();
        options.command[2] = "printf '  dummy-password  \\r\\n'".into();
        assert_eq!(secret(&options).unwrap(), "  dummy-password  ");
        options.command[2] = "printf 'secret\\r'".into();
        assert_eq!(secret(&options).unwrap(), "secret\r");
        options.command[2] = "printf '{\"totp\":\"012345\"}'".into();
        options.json_pointer = Some("/totp".into());
        assert_eq!(secret(&options).unwrap(), "012345");
        options.command[2] = "printf secret-on-stderr >&2; exit 1".into();
        assert!(!format!("{:#}", secret(&options).unwrap_err()).contains("secret-on-stderr"));
    }

    #[tokio::test]
    async fn origin_is_bound_to_fill_and_native_errors_do_not_disclose_secrets() {
        let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
        let app = Router::new()
            .route(
                "/tabs",
                get(|| async {
                    Json(json!({"tabs":[{"tabId":"a","url":"https://login.example.com/form"}]}))
                }),
            )
            .route(
                "/tabs/a/type",
                post(
                    |State(seen): State<Arc<Mutex<Vec<Value>>>>, Json(body): Json<Value>| async move {
                        let mut seen = seen.lock().unwrap();
                        let attempt = seen.len();
                        seen.push(body);
                        let (status, body) = match attempt {
                            0 => (StatusCode::CONFLICT, json!({"error":"echoed dummy-password"})),
                            1 => (StatusCode::OK, json!({"ok":"echoed dummy-password"})),
                            2 => (StatusCode::OK, json!({"ok":false,"error":"echoed dummy-password"})),
                            _ => (StatusCode::OK, json!({"ok":true})),
                        };
                        (status, Json(body))
                    },
                ),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint: Url = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let browser = browser(endpoint);
        let inspected = seen.clone();
        tokio::task::spawn_blocking(move || {
            for _ in 0..3 {
                let error = fill(&browser, &options()).unwrap_err();
                assert!(!format!("{error:#}").contains("dummy-password"));
            }
            assert_eq!(
                inspected.lock().unwrap()[0]["expectedOrigin"],
                "https://login.example.com"
            );
            assert_eq!(inspected.lock().unwrap()[0]["text"], "  dummy-password  ");
            assert_eq!(inspected.lock().unwrap()[0]["ref"], "e12");
            fill(&browser, &options()).unwrap();
            let mut options = options();
            options.origin = "https://other.example.com".into();
            options.command = vec!["this-command-must-not-run".into()];
            assert!(
                format!("{:#}", fill(&browser, &options).unwrap_err())
                    .contains("No matching browser page")
            );
            options.origin = "https://login.example.com".into();
            options.reference = "@@e12".into();
            assert!(
                fill(&browser, &options)
                    .unwrap_err()
                    .to_string()
                    .contains("input ref")
            );
        })
        .await
        .unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn redirected_tab_lists_never_run_the_source_or_send_credentials_elsewhere() {
        use axum::http::header;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let requests = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/tabs",
                get(|| async {
                    (
                        StatusCode::TEMPORARY_REDIRECT,
                        [(header::LOCATION, "/stolen")],
                    )
                }),
            )
            .route(
                "/stolen",
                get(|State(requests): State<Arc<AtomicUsize>>| async move {
                    requests.fetch_add(1, Ordering::SeqCst);
                    Json(json!({"tabs":[{"tabId":"a","url":"https://login.example.com"}]}))
                }),
            )
            .with_state(requests.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let browser = browser(
            format!("http://{}/", listener.local_addr().unwrap())
                .parse()
                .unwrap(),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        tokio::task::spawn_blocking(move || {
            let mut options = options();
            options.command = vec!["this-command-must-not-run".into()];
            assert!(
                fill(&browser, &options)
                    .unwrap_err()
                    .to_string()
                    .contains("HTTP 307")
            );
        })
        .await
        .unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        server.abort();
    }
}
