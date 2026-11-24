#!/usr/bin/env python3
"""Publish a folder to an owned Beacon site with the three-call flow.

usage: beacon_upload.py DIR SLUG
Needs BEACON_API_KEY. Creates the site, or a new version when the slug exists.
"""

import hashlib
import json
import mimetypes
import os
import subprocess
import sys
import urllib.error
import urllib.request

API = "https://beacon.host"
root, slug = sys.argv[1], sys.argv[2]
key = os.environ["BEACON_API_KEY"]
headers = {
    "Authorization": f"Bearer {key}",
    "X-Beacon-Client": "claude-code",
    "Content-Type": "application/json",
}


def call(method, path, body=None):
    req = urllib.request.Request(
        API + path,
        method=method,
        headers=headers,
        data=None if body is None else json.dumps(body).encode(),
    )
    try:
        with urllib.request.urlopen(req) as res:
            return res.status, json.loads(res.read() or b"null")
    except urllib.error.HTTPError as err:
        return err.code, err.read().decode()


files = []
for dirpath, _, names in os.walk(root):
    for name in sorted(names):
        full = os.path.join(dirpath, name)
        rel = os.path.relpath(full, root)
        digest = hashlib.sha256()
        with open(full, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                digest.update(chunk)
        ctype = mimetypes.guess_type(name)[0] or "application/octet-stream"
        files.append(
            {
                "path": rel,
                "size": os.path.getsize(full),
                "contentType": ctype,
                "hash": "sha256:" + digest.hexdigest(),
            }
        )

status, site = call("POST", "/v1/sites", {"slug": slug, "files": files})
if status == 409:
    status, site = call("POST", f"/v1/sites/{slug}/versions", {"files": files})
if status not in (200, 201):
    sys.exit(f"create failed: {status} {site}")

types = {f["path"]: f["contentType"] for f in files}
pending = site["uploads"]["pending"]
print(
    f"{len(pending)} to upload, {len(site['uploads']['skipped'])} skipped", flush=True
)
for item in pending:
    path = item["path"]
    subprocess.run(
        [
            "curl",
            "--fail-with-body",
            "-sS",
            "-X",
            "PUT",
            item["uploadUrl"],
            "-H",
            f"Content-Type: {types[path]}",
            "--data-binary",
            "@" + os.path.join(root, path),
            "-o",
            "/dev/null",
        ],
        check=True,
    )
    print("uploaded", path, flush=True)

status, done = call("POST", site["version"]["finalizeUrl"])
if status not in (200, 201):
    sys.exit(f"finalize failed: {status} {done}")
print(
    json.dumps({"url": f"https://{slug}.beacon.host", "version": site["version"]["id"]})
)
