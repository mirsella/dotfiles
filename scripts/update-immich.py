#!/usr/bin/env python3
"""Bump the Immich container pin to the latest same-major release.

Rewrites modules/nixos/immich.nix, normally via `nix run .#update-packages`.
"""

import json
import pathlib
import re
import sys
import urllib.error
import urllib.request

IMMICH_NIX = pathlib.Path("modules/nixos/immich.nix")
OWNER = "immich-app"
REPO = f"{OWNER}/immich"
IMAGES = ("immich-server", "immich-machine-learning")
REGISTRY = "https://ghcr.io"
SEMVER = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
COMPOSE_SERVICE = re.compile(
    rf"image:\s+((?:ghcr\.io/{OWNER}/postgres|docker\.io/valkey/valkey):\S+)"
)


def get(url, headers=None, method="GET"):
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "dotfiles-update-packages", **(headers or {})},
        method=method,
    )
    try:
        return urllib.request.urlopen(request)
    except urllib.error.URLError as error:
        fail(f"GET {url}: {error}")


def fail(message):
    print(f"immich: error: {message}", file=sys.stderr)
    sys.exit(1)


def parse_semver(tag):
    match = SEMVER.match(tag)
    return None if match is None else tuple(int(part) for part in match.groups())


def pick_release(current):
    """Newest stable release in the current major, else the newest stable major."""
    with get(
        f"https://api.github.com/repos/{REPO}/releases?per_page=30",
        {"Accept": "application/vnd.github+json"},
    ) as response:
        releases = json.load(response)
    versions = {}
    for release in releases:
        version = parse_semver(release["tag_name"])
        if not release["prerelease"] and not release["draft"] and version is not None:
            versions[version] = release["tag_name"]

    newer = [version for version in versions if version > current]
    same_major = [version for version in newer if version[0] == current[0]]
    if same_major:
        return versions[max(same_major)], None
    if newer:
        return None, versions[max(newer)]
    return None, None


def image_digest(name, tag):
    scope = f"repository:{OWNER}/{name}:pull"
    with get(f"{REGISTRY}/token?scope={scope}&service=ghcr.io") as response:
        token = json.load(response)["token"]
    with get(
        f"{REGISTRY}/v2/{OWNER}/{name}/manifests/{tag}",
        {
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.oci.image.index.v1+json,"
            "application/vnd.docker.distribution.manifest.list.v2+json",
        },
        method="HEAD",
    ) as response:
        return response.headers["Docker-Content-Digest"]


def substitute_once(text, pattern, replacement):
    updated, count = re.subn(pattern, replacement, text)
    if count != 1:
        fail(f"expected exactly one match for {pattern}")
    return updated


def main():
    try:
        text = IMMICH_NIX.read_text()
    except OSError as error:
        fail(f"cannot read {IMMICH_NIX}: {error}")
    current_match = re.search(r'version = "([^"]+)";', text)
    current_tag = current_match.group(1) if current_match else None
    current = parse_semver(current_tag or "")
    if current is None:
        fail(f"cannot find a semver version in {IMMICH_NIX}")

    tag, newer_major = pick_release(current)
    if tag is None:
        if newer_major is not None:
            print(
                f"immich: holding {current_tag}, newer major {newer_major} needs a manual bump",
                file=sys.stderr,
            )
        else:
            print(f"immich: {current_tag} is up to date")
        return

    with get(
        f"https://github.com/{REPO}/releases/download/{tag}/docker-compose.yml"
    ) as response:
        compose = response.read().decode()
    for image in COMPOSE_SERVICE.findall(compose):
        if image not in text:
            fail(f"{tag} pins {image}, not present in {IMMICH_NIX}; update manually")

    updated = substitute_once(text, r'(version = ")[^"]+(";)', rf"\g<1>{tag}\g<2>")
    for name in IMAGES:
        updated = substitute_once(
            updated,
            rf"(ghcr\.io/{OWNER}/{name}:\$\{{version\}}@)sha256:[0-9a-f]{{64}}",
            rf"\g<1>{image_digest(name, tag)}",
        )

    IMMICH_NIX.write_text(updated)
    print(f"immich: {current_tag} -> {tag}")


if __name__ == "__main__":
    main()
