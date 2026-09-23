"""Configure Beszel's PocketBase settings and local monitoring system."""

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

HUB = os.environ.get("BESZEL_HUB", "http://127.0.0.1:8090")
EMAIL = "mirsella@mirsella.mooo.com"
NAME = "mirsella"
SYSTEM = {"name": "predator", "host": "127.0.0.1", "port": "45876"}
SMTP = {
    "enabled": True,
    "host": "smtp.resend.com",
    "port": 465,
    "username": "resend",
    "authMethod": "PLAIN",
    "tls": True,
    "localName": "",
}
SENDER = {"senderName": "Beszel", "senderAddress": "noreply@voxride.com"}


class ApiError(Exception):
    """Non-2xx response from the hub API."""

    def __init__(self, method, path, error):
        self.status = error.code
        super().__init__(
            f"{method} {path}: HTTP {error.code}: {error.read().decode()[:200]}"
        )


def request(method, path, token=None, body=None, params=None):
    url = HUB + path
    if params:
        url += "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode() if body is not None else None,
        method=method,
        headers={"Content-Type": "application/json"},
    )
    if token:
        req.add_header("Authorization", token)
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            raw = r.read().decode()
            return json.loads(raw) if raw else None
    except urllib.error.HTTPError as e:
        raise ApiError(method, path, e) from e


def wait_for_hub():
    for _ in range(60):
        try:
            if request("GET", "/api/health"):
                return
        except OSError:
            pass
        time.sleep(2)
    sys.exit("beszel-setup: hub not reachable at " + HUB)


def ensure_record(token, collection, filt, body, describe, immutable=()):
    found = request(
        "GET",
        f"/api/collections/{collection}/records",
        token=token,
        params={"filter": filt},
    )
    if found["totalItems"]:
        record = found["items"][0]
        uid = record["id"]
        patch = {
            k: v for k, v in body.items() if k not in immutable and record.get(k) != v
        }
        if patch:
            request(
                "PATCH",
                f"/api/collections/{collection}/records/{uid}",
                token=token,
                body=patch,
            )
            print(f"beszel-setup: {describe} updated {uid}")
        else:
            print(f"beszel-setup: {describe} exists {uid}")
        return uid
    rec = request(
        "POST", f"/api/collections/{collection}/records", token=token, body=body
    )
    print(f"beszel-setup: {describe} created {rec['id']}")
    return rec["id"]


def read_secret(path):
    with open(path) as f:
        return f.read().strip()


def main(superuser_pw_file, resend_key_file):
    superuser_pw = read_secret(superuser_pw_file)
    resend_key = read_secret(resend_key_file)

    wait_for_hub()

    try:
        auth = request(
            "POST",
            "/api/collections/_superusers/auth-with-password",
            body={"identity": EMAIL, "password": superuser_pw},
        )
    except ApiError as e:
        if e.status not in (400, 401):
            raise
        sys.exit(
            "beszel-setup: superuser auth failed; bootstrap with "
            "`beszel-hub superuser upsert` while beszel-hub is stopped"
        )
    token = auth["token"]

    settings = request("GET", "/api/settings", token=token)
    patch = {}
    if any(settings["smtp"].get(k) != v for k, v in SMTP.items()):
        patch["smtp"] = {**SMTP, "password": resend_key}
    if any(settings["meta"].get(k) != v for k, v in SENDER.items()):
        patch["meta"] = {**settings["meta"], **SENDER}
    if patch:
        request("PATCH", "/api/settings", token=token, body=patch)
        print("beszel-setup: settings patched", sorted(patch))
    else:
        print("beszel-setup: settings already converged")

    uid = ensure_record(
        token,
        "users",
        f'email = "{EMAIL}"',
        {
            "email": EMAIL,
            "password": superuser_pw,
            "passwordConfirm": superuser_pw,
            "name": NAME,
            "username": NAME,
            "role": "admin",
            "verified": True,
        },
        "hub user",
        immutable=("password", "passwordConfirm"),
    )
    ensure_record(
        token,
        "systems",
        f'name = "{SYSTEM["name"]}"',
        {**SYSTEM, "users": [uid]},
        "system",
    )


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
