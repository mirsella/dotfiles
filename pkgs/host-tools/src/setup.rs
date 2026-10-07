use crate::util;
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use ureq::http::Method;

#[cfg(test)]
mod tests;

struct BeszelApi {
    agent: ureq::Agent,
    hub: String,
    token: String,
}

fn response_json(mut response: ureq::http::Response<ureq::Body>) -> Result<Value> {
    if !response.status().is_success() {
        return Err(ureq::Error::StatusCode(response.status().as_u16()).into());
    }
    let raw = response.body_mut().read_to_string()?;
    if raw.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(serde_json::from_str(&raw)?)
    }
}

fn record_patch(
    record: &Value,
    desired: &Value,
    immutable: &[&str],
) -> Result<serde_json::Map<String, Value>> {
    let mut patch = serde_json::Map::new();
    for (key, value) in desired
        .as_object()
        .context("record body is not an object")?
    {
        if !immutable.contains(&key.as_str()) && record.get(key) != Some(value) {
            patch.insert(key.clone(), value.clone());
        }
    }
    Ok(patch)
}

impl BeszelApi {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let response = self
            .agent
            .get(format!("{}{path}", self.hub))
            .header("Authorization", &self.token)
            .query_pairs(query.iter().copied())
            .call()
            .with_context(|| format!("GET {path}"))?;
        response_json(response).with_context(|| format!("GET {path}"))
    }

    fn write(&self, method: Method, path: &str, body: Value) -> Result<Value> {
        let request = ureq::http::Request::builder()
            .method(&method)
            .uri(format!("{}{path}", self.hub))
            .header("Authorization", &self.token)
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&body)?)?;
        let response = self
            .agent
            .run(request)
            .with_context(|| format!("{method} {path}"))?;
        response_json(response).with_context(|| format!("{method} {path}"))
    }

    fn ensure_record(
        &self,
        collection: &str,
        filter: &str,
        body: Value,
        immutable: &[&str],
    ) -> Result<String> {
        let base = format!("/api/collections/{collection}/records");
        let found = self.get(&base, &[("filter", filter)])?;
        let count = found["totalItems"]
            .as_u64()
            .context("missing record count")?;
        ensure!(
            count <= 1,
            "ambiguous {collection} configuration: {count} records match"
        );
        if count == 1 {
            let record = found["items"]
                .as_array()
                .and_then(|a| a.first())
                .context("missing matching record")?;
            let id = record["id"].as_str().context("missing record ID")?;
            let patch = record_patch(record, &body, immutable)?;
            if !patch.is_empty() {
                self.write(Method::PATCH, &format!("{base}/{id}"), Value::Object(patch))?;
            }
            println!("beszel-setup: {collection} converged {id}");
            Ok(id.into())
        } else {
            let record = self.write(Method::POST, &base, body)?;
            Ok(record["id"]
                .as_str()
                .context("missing created record ID")?
                .into())
        }
    }
}

pub fn beszel(password: &Path, resend: &Path) -> Result<()> {
    let password = util::read(password)?.trim().to_owned();
    let resend = util::read(resend)?.trim().to_owned();
    ensure!(
        !password.is_empty() && !resend.is_empty(),
        "empty Beszel credential"
    );
    let mut api = BeszelApi {
        hub: std::env::var("BESZEL_HUB").unwrap_or_else(|_| "http://127.0.0.1:8090".into()),
        agent: util::agent(30),
        token: String::new(),
    };
    for attempt in 0..60 {
        match api.get("/api/health", &[]) {
            Ok(_) => break,
            Err(e) if attempt == 59 => return Err(e).context("Beszel hub did not become ready"),
            Err(e) => {
                if attempt == 0 {
                    eprintln!("Waiting for Beszel health: {e}");
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
    let email = "mirsella@mirsella.mooo.com";
    let auth = match api.write(
        Method::POST,
        "/api/collections/_superusers/auth-with-password",
        json!({"identity":email,"password":password}),
    ) {
        Ok(auth) => auth,
        Err(e)
            if matches!(
                e.downcast_ref::<ureq::Error>(),
                Some(ureq::Error::StatusCode(400 | 401))
            ) =>
        {
            bail!(
                "beszel-setup: superuser auth failed; bootstrap with `beszel-hub superuser upsert` while beszel-hub is stopped"
            )
        }
        Err(e) => return Err(e),
    };
    api.token = auth["token"]
        .as_str()
        .context("missing Beszel auth token")?
        .to_owned();
    api.write(
        Method::PATCH,
        "/api/settings",
        json!({
            "smtp":{"enabled":true,"host":"smtp.resend.com","port":465,"username":"resend","authMethod":"PLAIN","tls":true,"localName":"","password":resend},
            "meta":{"senderName":"Beszel","senderAddress":"noreply@voxride.com"}
        }),
    )?;
    let uid = api.ensure_record(
        "users",
        &format!("email = \"{email}\""),
        json!({"email":email,"password":password,"passwordConfirm":password,"name":"mirsella","username":"mirsella","role":"admin","verified":true}),
        &["password", "passwordConfirm"],
    )?;
    api.ensure_record(
        "systems",
        "name = \"predator\"",
        json!({"name":"predator","host":"127.0.0.1","port":"45876","users":[uid]}),
        &[],
    )?;
    println!("beszel-setup: mail, user and system configured");
    Ok(())
}

#[derive(Deserialize)]
struct Mount {
    name: String,
    path: String,
    users: Vec<String>,
}
#[derive(Deserialize)]
struct Nextcloud {
    sharing: PathBuf,
    mounts: Vec<Mount>,
}

pub fn validate_mount(mount: &Value, expected_path: &str) -> Result<()> {
    ensure!(
        mount["storage"] == "\\OC\\Files\\Storage\\Local"
            && mount["authentication_type"] == "null::null"
            && mount["configuration"]["datadir"] == expected_path,
        "Unexpected external mount configuration"
    );
    Ok(())
}

pub fn nextcloud(path: &Path) -> Result<()> {
    let config: Nextcloud = serde_json::from_str(&util::read(path)?)?;
    util::run(
        "nextcloud-occ",
        &[
            "app:enable",
            "files_external",
            "twofactor_backupcodes",
            "twofactor_totp",
            "suspicious_login",
        ],
    )?;
    util::run(
        "nextcloud-occ",
        &["config:import", util::path(&config.sharing)?],
    )?;
    for option in [
        "shareapi_enable_link_password_by_default",
        "shareapi_enforce_links_password",
        "shareapi_default_expire_date",
        "shareapi_enforce_expire_date",
    ] {
        util::run(
            "nextcloud-occ",
            &[
                "config:app:set",
                "core",
                option,
                "--type=boolean",
                "--value=false",
            ],
        )?;
    }
    let mounts = util::json("nextcloud-occ", &["files_external:list", "--output=json"])?;
    let mounts = mounts.as_array().context("invalid external mount list")?;
    for expected in config.mounts {
        let name = format!("/{}", expected.name);
        let mut found = mounts.iter().filter(|v| v["mount_point"] == name);
        let existing = found.next();
        ensure!(found.next().is_none(), "duplicate external mount {name}");
        if let Some(mount) = existing {
            validate_mount(mount, &expected.path)?;
        } else {
            let datadir = format!("datadir={}", expected.path);
            let mut args = vec![
                "files_external:create",
                &name,
                "local",
                "null::null",
                "--config",
                &datadir,
            ];
            let users: Vec<_> = expected
                .users
                .iter()
                .map(|u| format!("--applicable-user={u}"))
                .collect();
            args.extend(users.iter().map(String::as_str));
            util::run("nextcloud-occ", &args)?;
        }
    }
    let mounts = util::json("nextcloud-occ", &["files_external:list", "--output=json"])?;
    for mount in mounts.as_array().context("invalid external mount list")? {
        let id = match &mount["mount_id"] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => bail!("missing mount ID"),
        };
        util::run(
            "nextcloud-occ",
            &["files_external:option", &id, "enable_sharing", "true"],
        )?;
    }
    Ok(())
}
