//! NASA's replacement APOD API, also used by the fixed upstream Plasma provider.
use crate::util;
use anyhow::{Context, Result, bail, ensure};
use dbus::blocking::Connection;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf, time::Duration};
use url::Url;

const FEED: &str = "https://science.nasa.gov/wp-json/wp/v2/apod-basic?per_page=10";

#[derive(Debug, Deserialize)]
struct Picture {
    date: String,
    title: String,
    media_type: String,
    hdurl: Option<String>,
}

fn latest_image(pictures: &[Picture]) -> Result<&Picture> {
    for picture in pictures {
        match picture.media_type.as_str() {
            "image" => return Ok(picture),
            "video" => eprintln!(
                "nasa-wallpaper: {} is a video; using the latest still image",
                picture.date
            ),
            other => bail!("unexpected APOD media type: {other}"),
        }
    }
    bail!("NASA feed contains no still images")
}

fn image_url(picture: &Picture) -> Result<Url> {
    let url = Url::parse(
        picture
            .hdurl
            .as_deref()
            .context("APOD image has no hdurl")?,
    )?;
    ensure!(
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host.ends_with(".nasa.gov"))
            && url.username().is_empty()
            && url.password().is_none(),
        "APOD image must use HTTPS on a NASA host: {url}"
    );
    Ok(url)
}

fn fetch(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    ensure!(
        response.status().is_success(),
        "GET {url}: {}",
        response.status()
    );
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "GET {url}: response exceeds {limit} bytes"
    );
    Ok(bytes)
}

fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
}

fn wallpaper_script(image: &Url) -> Result<String> {
    let image = serde_json::to_string(image.as_str())?;
    Ok(format!(
        r#"var image = {image};
var ds = desktops();
for (var i = 0; i < ds.length; i++) {{
    var d = ds[i];
    d.wallpaperPlugin = "org.kde.image";
    d.currentConfigGroup = ["Wallpaper", "org.kde.image", "General"];
    if (d.readConfig("Image", "") !== image) d.writeConfig("Image", image);
    if (d.readConfig("FillMode", -1) !== 2) d.writeConfig("FillMode", 2);
}}
print(JSON.stringify(ds.length));"#
    ))
}

pub fn run() -> Result<()> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .context("XDG_CACHE_HOME and HOME are unset")?
        .join("nasa-wallpaper");
    fs::create_dir_all(&cache)?;
    let agent = util::agent(60);
    let pictures: Vec<Picture> = serde_json::from_slice(&fetch(&agent, FEED, 2 * 1024 * 1024)?)?;
    let picture = latest_image(&pictures)?;
    let url = image_url(picture)?;
    let filename = format!("apod-{:x}.image", Sha256::digest(url.as_str().as_bytes()));
    let image = cache.join(&filename);
    if !image.exists() {
        let bytes = fetch(&agent, url.as_str(), 64 * 1024 * 1024)?;
        ensure!(is_image(&bytes), "NASA did not return a supported image");
        util::atomic_write(&image, &bytes)?;
    }

    // The chezmoi template uses this stable alias; the live desktop uses the
    // immutable filename so Qt never reuses yesterday's cached texture.
    let temporary = tempfile::tempdir_in(&cache)?;
    std::os::unix::fs::symlink(&filename, temporary.path().join("current.jpg"))?;
    fs::rename(
        temporary.path().join("current.jpg"),
        cache.join("current.jpg"),
    )?;
    let image_url =
        Url::from_file_path(&image).map_err(|_| anyhow::anyhow!("cache path is not absolute"))?;
    let connection = Connection::new_session()?;
    let proxy = connection.with_proxy(
        "org.kde.plasmashell",
        "/PlasmaShell",
        Duration::from_secs(30),
    );
    let (reply,): (String,) = proxy.method_call(
        "org.kde.PlasmaShell",
        "evaluateScript",
        (wallpaper_script(&image_url)?,),
    )?;
    let desktops: usize =
        serde_json::from_str(reply.trim()).context("Plasma rejected the wallpaper script")?;
    ensure!(desktops > 0, "Plasma has no desktops to update");
    println!(
        "nasa-wallpaper: {} — {} ({desktops} desktops)",
        picture.date, picture.title
    );

    // Keep only our current download, leaving unrelated cache files alone.
    for entry in fs::read_dir(&cache)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name != filename
            && name
                .strip_prefix("apod-")
                .and_then(|s| s.strip_suffix(".image"))
                .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pictures() -> Vec<Picture> {
        serde_json::from_str(r#"[
            {"date":"2026-11-11","title":"Video day","media_type":"video","hdurl":null},
            {"date":"2026-11-10","title":"Saturn Behind the Moon","media_type":"image","hdurl":"https://assets.science.nasa.gov/moon.jpg"}
        ]"#).unwrap()
    }

    #[test]
    fn video_days_use_the_latest_still_image() {
        let pictures = pictures();
        assert_eq!(latest_image(&pictures).unwrap().date, "2026-11-10");
        assert!(latest_image(&pictures[..1]).is_err());
        assert!(latest_image(&[]).is_err());
    }

    #[test]
    fn malformed_images_and_unknown_media_fail_explicitly() {
        let mut pictures = pictures();
        pictures[0].media_type = "unexpected".into();
        assert!(latest_image(&pictures).is_err());
        pictures[1].hdurl = None;
        assert!(image_url(&pictures[1]).is_err());
        for url in [
            "http://assets.science.nasa.gov/a.jpg",
            "https://not-nasa.gov/a.jpg",
            "https://nasa.gov.attacker.test/a.jpg",
        ] {
            pictures[1].hdurl = Some(url.into());
            assert!(image_url(&pictures[1]).is_err());
        }
    }

    #[test]
    fn reject_html_instead_of_replacing_the_last_good_image() {
        assert!(is_image(b"\xff\xd8\xffimage"));
        assert!(is_image(b"\x89PNG\r\n\x1a\nimage"));
        assert!(is_image(b"RIFFxxxxWEBPimage"));
        assert!(!is_image(b"<html>temporarily unavailable</html>"));
        assert!(!is_image(b""));
    }

    #[test]
    fn script_escapes_paths_and_targets_every_desktop() {
        let url = Url::from_file_path("/home/user/a'\"b/image.jpg").unwrap();
        let script = wallpaper_script(&url).unwrap();
        assert!(script.contains(&serde_json::to_string(url.as_str()).unwrap()));
        assert!(script.contains("var ds = desktops()"));
        assert!(script.contains("d.writeConfig(\"FillMode\", 2)"));
    }
}
