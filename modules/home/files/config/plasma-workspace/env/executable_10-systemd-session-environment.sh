#!/bin/sh

# Keep user services in the same environment as the Plasma session.
/usr/bin/dbus-update-activation-environment --systemd --all
