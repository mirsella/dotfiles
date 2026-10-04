"""Check real kernel messages, journal checkpoints, and delivery retries."""

import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tracemalloc
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "hosts/predator"))
spec = importlib.util.spec_from_file_location(
    "storage_alerts",
    Path(__file__).resolve().parents[1] / "hosts/predator/storage-alerts.py",
)
alerts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(alerts)

DISCONNECT = "usb 2-2: USB disconnect, device number 2"
CACHE_FAILURE = (
    "sd 3:0:0:0: [sdc] Synchronize Cache(10) failed: "
    "Result: hostbyte=DID_ERROR driverbyte=DRIVER_OK"
)


def entry(cursor, message):
    return {
        "__CURSOR": cursor,
        "__REALTIME_TIMESTAMP": "1790521952000000",
        "MESSAGE": message,
    }


class StorageAlerts(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.cursor = Path(directory.name) / "cursor"
        self.journal = self.enterContext(patch.object(alerts, "journal_entries"))
        self.send = self.enterContext(patch.object(alerts, "send_message"))

    def test_disk_disconnect_reset_and_io_errors_match(self):
        for message in [
            DISCONNECT,
            CACHE_FAILURE,
            "usb 2-4: reset SuperSpeed USB device number 3 using xhci_hcd",
            "usb 2-2.1: USB disconnect, device number 5",
            "usb 2-3.3: USB disconnect, device number 6",
            "usb 2-3: USB disconnect, device number 4",
            "usb 2-2: device descriptor read/64, error -71",
            "sd 3:0:0:0: [sdc] tag#0 uas_eh_abort_handler 0 uas-tag 1 inflight: CMD",
            "I/O error, dev sde, sector 123 op 0x1:(WRITE)",
            "Buffer I/O error on dev dm-3, logical block 123",
            "xhci_hcd 0000:00:14.0: xHCI host controller not responding, assume dead",
        ]:
            with self.subTest(message=message):
                self.assertRegex(message, alerts.STORAGE_EVENT)

    def test_normal_boot_and_unrelated_usb_resets_do_not_alert(self):
        for message in [
            "usb 1-9: reset high-speed USB device number 3 using xhci_hcd",
            "usb 2-20: USB disconnect, device number 9",
            "usb 2-2: new SuperSpeed USB device number 2 using xhci_hcd",
            "sd 3:0:0:0: [sdc] Synchronizing SCSI cache",
            "PM: suspend exit",
        ]:
            with self.subTest(message=message):
                self.assertNotRegex(message, alerts.STORAGE_EVENT)

    def test_first_install_skips_historical_events(self):
        self.journal.return_value = [entry("initial", DISCONNECT)]
        alerts.check(self.cursor)
        self.journal.assert_called_once_with(None)
        self.send.assert_not_called()
        self.assertEqual(self.cursor.read_text(), "initial\n")

    def test_related_events_are_grouped_and_cursor_advances_past_normal_lines(self):
        self.cursor.write_text("previous-boot\n")
        self.journal.return_value = [
            entry("disconnect", DISCONNECT),
            entry("flush", CACHE_FAILURE),
            entry(
                "new-boot", "usb 2-2: new SuperSpeed USB device number 2 using xhci_hcd"
            ),
        ]
        alerts.check(self.cursor)
        self.journal.assert_called_once_with("previous-boot")
        self.send.assert_called_once()
        self.assertIn(DISCONNECT, self.send.call_args.args[0])
        self.assertIn(CACHE_FAILURE, self.send.call_args.args[0])
        self.assertEqual(self.cursor.read_text(), "new-boot\n")

    def test_delivery_failure_preserves_pending_events_for_retry(self):
        self.cursor.write_text("before\n")
        self.journal.return_value = [entry("after", DISCONNECT)]
        self.send.side_effect = RuntimeError("Telegram unavailable")
        with self.assertRaises(RuntimeError):
            alerts.check(self.cursor)
        self.assertEqual(self.cursor.read_text(), "before\n")
        self.send.side_effect = None
        alerts.check(self.cursor)
        self.assertEqual(self.cursor.read_text(), "after\n")

    def test_inaccessible_kernel_journal_fails_visibly(self):
        self.journal.return_value = []
        with self.assertRaisesRegex(RuntimeError, "journal permissions"):
            alerts.check(self.cursor)
        self.assertFalse(self.cursor.exists())

    def test_large_backlog_is_grouped_with_bounded_details(self):
        self.cursor.write_text("before\n")
        self.journal.return_value = (
            entry(str(i), DISCONNECT + "🙂" * 500) for i in range(10_000)
        )
        alerts.check(self.cursor)
        self.send.assert_called_once()
        text = self.send.call_args.args[0]
        self.assertLessEqual(len(text.encode("utf-16-le")) // 2, 4096)
        self.assertIn("Matching kernel messages: 10000", text)
        self.assertIn("Details truncated", text)
        self.assertEqual(self.cursor.read_text(), "9999\n")

    def test_binary_kernel_messages_are_not_silently_dropped(self):
        self.cursor.write_text("before\n")
        self.journal.return_value = [
            entry("after", list(DISCONNECT.encode() + b"\xff"))
        ]
        alerts.check(self.cursor)
        self.assertIn(DISCONNECT, self.send.call_args.args[0])
        self.assertEqual(self.cursor.read_text(), "after\n")

    def test_single_oversized_message_has_bounded_formatting_memory(self):
        self.cursor.write_text("before\n")
        self.journal.return_value = [entry("after", DISCONNECT + "🙂" * 1_000_000)]
        tracemalloc.start()
        try:
            alerts.check(self.cursor)
            _, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
        self.assertLess(peak, 1024 * 1024)
        text = self.send.call_args.args[0]
        self.assertLessEqual(len(text.encode("utf-16-le")), alerts.MESSAGE_BYTES)
        self.assertIn("Details truncated", text)
        self.assertEqual(self.cursor.read_text(), "after\n")

    def test_empty_checkpoint_does_not_reset_to_journal_tail(self):
        self.cursor.write_text("\n")
        with self.assertRaisesRegex(ValueError, "Empty journal checkpoint"):
            alerts.check(self.cursor)
        self.journal.assert_not_called()
        self.assertEqual(self.cursor.read_text(), "\n")

    def test_partial_journal_failure_neither_sends_nor_acknowledges(self):
        def broken_journal():
            yield entry("after", DISCONNECT)
            raise subprocess.CalledProcessError(1, "journalctl")

        self.cursor.write_text("before\n")
        self.journal.return_value = broken_journal()
        with self.assertRaises(subprocess.CalledProcessError):
            alerts.check(self.cursor)
        self.send.assert_not_called()
        self.assertEqual(self.cursor.read_text(), "before\n")


class JournalReader(unittest.TestCase):
    def test_streams_complete_fields_and_reports_process_failure(self):
        event = entry("after", DISCONNECT + "x" * 5000)
        with patch.object(alerts.subprocess, "Popen") as start:
            process = start.return_value.__enter__.return_value
            process.stdout = io.StringIO(json.dumps(event) + "\n")
            process.wait.return_value = process.returncode = 1
            entries = alerts.journal_entries("before")
            self.assertEqual(next(entries), event)
            with self.assertRaises(subprocess.CalledProcessError):
                next(entries)
            command = start.call_args.args[0]
            self.assertIn("--all", command)
            self.assertIn("--after-cursor=before", command)
            self.assertNotIn("--lines=1", command)


class TelegramDelivery(unittest.TestCase):
    def setUp(self):
        self.enterContext(patch.dict(os.environ, TgToken="secret-token", TgId="1234"))
        self.urlopen = self.enterContext(patch.object(alerts, "urlopen"))

    def test_success_posts_to_configured_chat(self):
        response = io.BytesIO(b'{"ok":true}')
        response.status = 200
        self.urlopen.return_value = response
        alerts.send_message("test")
        request = self.urlopen.call_args.args[0]
        self.assertEqual(json.loads(request.data), {"chat_id": "1234", "text": "test"})

    def test_api_failure_preserves_diagnostic_without_leaking_token(self):
        self.urlopen.side_effect = HTTPError(
            "https://api.telegram.org/botsecret-token/sendMessage",
            400,
            "Bad Request",
            {},
            io.BytesIO(b'{"ok":false,"description":"bad chat via secret-token"}'),
        )
        with self.assertRaisesRegex(
            RuntimeError, r"Telegram HTTP 400: bad chat via \[redacted\]"
        ):
            alerts.send_message("test")

    def test_non_json_gateway_failure_reports_http_status(self):
        self.urlopen.side_effect = HTTPError(
            "https://api.telegram.org/botsecret-token/sendMessage",
            502,
            "Bad Gateway",
            {},
            io.BytesIO(b"<html>Bad gateway</html>"),
        )
        with self.assertRaisesRegex(RuntimeError, "Telegram HTTP 502: invalid JSON"):
            alerts.send_message("test")

    def test_network_failure_does_not_leak_token(self):
        self.urlopen.side_effect = URLError("connection to botsecret-token failed")
        with self.assertRaisesRegex(
            RuntimeError, r"connection to bot\[redacted\] failed"
        ):
            alerts.send_message("test")


if __name__ == "__main__":
    unittest.main()
