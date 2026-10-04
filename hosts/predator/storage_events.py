"""Kernel storage faults shared by notifications and suspend safety checks."""

import re


# Physical USB storage ports, including the former hub port; sdX names vary.
STORAGE_EVENT = re.compile(
    r"\busb 2-(?:2|3|4)(?:\.\d+)*: (?:USB disconnect\b|reset .* USB device\b|"
    r"device descriptor read/.*error|device not accepting address|"
    r"unable to enumerate USB device)|"
    r"\buas_(?:eh_\w+|zap_pending)\b|"
    r"\bsd \S+: \[sd[a-z]+\] Synchronize Cache.*failed|"
    r"\bI/O error.*\bdev (?:sd[a-z]+\d*|dm-\d+)\b|"
    r"\bxhci_hcd\b.*(?:HC died|host controller not responding)"
)
