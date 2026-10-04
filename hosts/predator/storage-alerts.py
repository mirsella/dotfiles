"""Poll the persistent kernel journal; acknowledge events only after delivery."""

import argparse
from datetime import datetime
import json
import os
from pathlib import Path
import socket
import subprocess
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from storage_events import STORAGE_EVENT


MESSAGE_BYTES = 4096 * 2  # Telegram's limit measured in UTF-16 code units.


def journal_entries(cursor):
    args = [
        "journalctl",
        "--quiet",
        "--no-pager",
        "--all",  # Otherwise journalctl replaces large JSON fields with null.
        "--output=json",
        "--output-fields=__CURSOR,__REALTIME_TIMESTAMP,MESSAGE",
        "_TRANSPORT=kernel",
    ]
    args.append("--lines=1" if cursor is None else f"--after-cursor={cursor}")
    with subprocess.Popen(args, stdout=subprocess.PIPE, text=True) as journal:
        for line in journal.stdout:
            yield json.loads(line)
        if journal.wait():
            raise subprocess.CalledProcessError(journal.returncode, args)


def send_message(text):
    token = os.environ["TgToken"]
    request = Request(
        f"https://api.telegram.org/bot{token}/sendMessage",
        data=json.dumps({"chat_id": os.environ["TgId"], "text": text}).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        response = urlopen(request, timeout=20)
    except HTTPError as error:
        response = error  # Bot API errors carry useful JSON descriptions too.
    except URLError as error:
        raise RuntimeError(
            f"Telegram connection failed: {error.reason}".replace(token, "[redacted]")
        ) from None
    with response:
        try:
            result = json.load(response)
        except (json.JSONDecodeError, UnicodeDecodeError):
            raise RuntimeError(
                f"Telegram HTTP {response.status}: invalid JSON response"
            ) from None
    if not result["ok"]:
        raise RuntimeError(
            f"Telegram HTTP {response.status}: {result['description']}".replace(
                token, "[redacted]"
            )
        )


def check(cursor_file):
    try:
        cursor = cursor_file.read_text().strip()
    except FileNotFoundError:
        cursor = None
    else:
        if not cursor:
            raise ValueError(f"Empty journal checkpoint: {cursor_file}")

    last_cursor = None
    count = 0
    details = bytearray()
    for entry in journal_entries(cursor):
        last_cursor = entry["__CURSOR"]
        if cursor is None:
            continue
        message = entry["MESSAGE"]
        # journalctl represents non-UTF-8 MESSAGE fields as arrays of bytes.
        if isinstance(message, list):
            message = bytes(message).decode("utf-8", errors="replace")
        if STORAGE_EVENT.search(message):
            count += 1
            if len(details) < MESSAGE_BYTES:
                stamp = (
                    datetime.fromtimestamp(
                        int(entry["__REALTIME_TIMESTAMP"]) / 1_000_000
                    )
                    .astimezone()
                    .isoformat(timespec="seconds")
                )
                remaining = MESSAGE_BYTES - len(details)
                line = f"\n\n{stamp}  {message[: remaining // 2]}".encode("utf-16-le")
                details.extend(line[:remaining])

    if last_cursor is None:
        if cursor is None:
            raise RuntimeError("No kernel journal entries; check journal permissions")
        return
    if cursor is None:
        print(
            "Monitoring starts at the current journal tail; historical events skipped."
        )
    if count:
        text = (
            f"Storage connection alert on {socket.gethostname()}\nMatching kernel messages: {count}"
        ).encode("utf-16-le") + details
        if len(text) > MESSAGE_BYTES:
            suffix = "\n[Details truncated; see kernel journal.]".encode("utf-16-le")
            text = text[: MESSAGE_BYTES - len(suffix)] + suffix
        # Truncation can split a surrogate pair; omit only that incomplete character.
        send_message(text.decode("utf-16-le", errors="ignore"))
        print(f"Telegram notification delivered for {count} storage events.")

    # On delivery failure the old cursor remains, so the next timer run retries.
    temporary = cursor_file.with_suffix(".tmp")
    temporary.write_text(last_cursor + "\n")
    temporary.replace(cursor_file)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test", action="store_true")
    args = parser.parse_args()
    if args.test:
        send_message(
            f"Storage alert test from {socket.gethostname()}. Telegram delivery works."
        )
        print("Telegram test delivered.")
    else:
        check(Path(os.environ["STATE_DIRECTORY"]) / "cursor")


if __name__ == "__main__":
    main()
