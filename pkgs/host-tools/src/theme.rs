//! Keep the GTK settings portal's color scheme aligned with Plasma when
//! kde-gtk-config stops mirroring the palette. Only repair genuine mismatches:
//! synthetic opposite-theme pulses visibly flash every listening application.

use crate::util;
use anyhow::{Context, Result};
use std::{fs, path::PathBuf, thread, time::Duration};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const SCHEME_KEY: &str = "/org/gnome/desktop/interface/color-scheme";

pub fn run(dconf: &str) -> Result<()> {
    loop {
        if let Err(error) = reconcile(dconf) {
            eprintln!("electron-theme-watch: {error:#}; retrying");
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn reconcile(dconf: &str) -> Result<()> {
    let desired = plasma_scheme().context("cannot read the Plasma color palette")?;
    let current = util::output(dconf, &["read", SCHEME_KEY])?;
    if current.trim() != desired {
        println!("electron-theme-watch: repairing portal scheme to {desired}");
        util::run(dconf, &["write", SCHEME_KEY, desired])?;
    }
    Ok(())
}

/// The scheme the portal must serve, taken from the Plasma palette in
/// kdeglobals: the resolved colors Plasma itself renders with, independent of
/// whether kde-gtk-config's kded module is alive to mirror them.
fn plasma_scheme() -> Option<&'static str> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    scheme_for_palette(&fs::read_to_string(config.join("kdeglobals")).ok()?)
}

/// Decide dark or light from a kdeglobals palette. Window is what the desktop
/// renders against; View and Button answer when it is missing.
fn scheme_for_palette(kdeglobals: &str) -> Option<&'static str> {
    for group in ["Colors:Window", "Colors:View", "Colors:Button"] {
        if let Some(rgb) = palette_background(kdeglobals, group) {
            return Some(if is_dark(rgb) {
                "'prefer-dark'"
            } else {
                "'prefer-light'"
            });
        }
    }
    None
}

fn palette_background(kdeglobals: &str, group: &str) -> Option<(u32, u32, u32)> {
    let mut in_group = false;
    for line in kdeglobals.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = section == group;
        } else if in_group {
            if let Some(value) = line.strip_prefix("BackgroundNormal=") {
                let mut channels = value
                    .trim()
                    .split(',')
                    .map(|c| c.trim().parse::<u8>().ok().map(u32::from));
                let rgb = (channels.next()??, channels.next()??, channels.next()??);
                return channels.next().is_none().then_some(rgb);
            }
        }
    }
    None
}

/// Rec. 709 luma of a Qt color triple on a 0-255 scale; dark below 128.
fn is_dark((red, green, blue): (u32, u32, u32)) -> bool {
    2126 * red + 7152 * green + 722 * blue < 1_280_000
}

#[cfg(test)]
mod tests {
    use super::{is_dark, scheme_for_palette};

    #[test]
    fn palette_scheme_follows_the_kde_background() {
        let cases = [
            // The palette Plasma itself renders with, Breeze dark vs light values.
            (
                "[Colors:Window]\nBackgroundNormal=32,35,38\nForegroundNormal=252,252,252\n[ColorEffects:Disabled]\nColor=56,56,56\n",
                Some("'prefer-dark'"),
            ),
            (
                "[Colors:Window]\nBackgroundNormal=239,240,243\nForegroundNormal=35,38,41\n",
                Some("'prefer-light'"),
            ),
            // View and Button answer when Window is missing, in that order.
            (
                "[Colors:View]\nBackgroundNormal=20,22,24\n",
                Some("'prefer-dark'"),
            ),
            (
                "[Colors:Button]\nBackgroundNormal=239,240,243\n",
                Some("'prefer-light'"),
            ),
            // No palette for the group, or no palette at all: no verdict.
            ("[Colors:Window]\nForegroundNormal=1,2,3\n", None),
            ("", None),
            // Keys before the target group belong to no group and never answer;
            // kdeglobals places [General] among the color groups.
            (
                "BackgroundNormal=255,255,255\n[Colors:Button]\nBackgroundNormal=32,35,38\n",
                Some("'prefer-dark'"),
            ),
            // A malformed triple is no verdict rather than a wrong one.
            ("[Colors:Window]\nBackgroundNormal=32,35\n", None),
            ("[Colors:Window]\nBackgroundNormal=32,35,38,1\n", None),
            ("[Colors:Window]\nBackgroundNormal=256,35,38\n", None),
            // Mid gray lands on light; the threshold is exclusive.
            (
                "[Colors:Window]\nBackgroundNormal=128,128,128\n",
                Some("'prefer-light'"),
            ),
        ];
        for (kdeglobals, expected) in cases {
            assert_eq!(
                scheme_for_palette(kdeglobals),
                expected,
                "for {kdeglobals:?}"
            );
        }
    }

    #[test]
    fn luma_threshold_splits_dark_from_light() {
        assert!(is_dark((0, 0, 0)));
        assert!(!is_dark((255, 255, 255)));
    }
}
