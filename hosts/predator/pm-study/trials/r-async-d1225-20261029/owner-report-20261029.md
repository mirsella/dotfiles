# Owner confirmation on October 29, 2026

For `r-async-d1225-20261029`, after reporting recovery, the owner stated:

> it was stuck, 15w etc. the usual. power button hold.

Verdict: **owner-confirmed HANG**. The approximately 15 W draw is owner-reported, not an independently retrieved measurement or dated graph. The owner used a power-button hold to recover the machine. The subsequent startup method and exact recovery timings were not specified.

The recovery boot independently observed at 13:20:27 CET is `4e5cb3ba-062b-4ad0-bd18-7e24492f97e6`, with boot time October 29 at 12:05:10 CET. It does not establish a successful return from the earlier suspend. Retained old-boot artifacts contain no return, result or completed record; measured sleep duration and the full dated graph remain unavailable.

This report establishes the physical hang outcome, not a failing driver or precise suspend phase. No agent recovery WoL, reset or power toggle was performed.
