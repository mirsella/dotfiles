#!/usr/bin/env bash

# Yakuake is a drop-down terminal emulator based on KDE Konsole technology.
# KDE's default terminal emulator supports profiles, you can create one in
# Settings > Manage Profiles. You can select a dark or light theme in
# Appearance > Color scheme and font. The following script iterates over all
# instances of Konsole und changes the profile of all sessions. This is necessary,
# if there are multiple tabs in one of the Konsole instances.
# Reference: https://docs.kde.org/stable5/en/konsole/konsole/konsole.pdf

PROFILE='Light'
# get number of sessions running within Yakuake
SESSIONIDS=$(qdbus org.kde.yakuake /Sessions org.freedesktop.DBus.Introspectable.Introspect | grep -o '<node name="[0-9]\+"/>' | grep -o '[0-9]\+')
for ID in $SESSIONIDS; do
	# change profile through dbus message
	qdbus org.kde.yakuake /Sessions/$ID setProfile "$PROFILE"
	qdbus org.kde.yakuake /Windows/$ID setDefaultProfile "$PROFILE"
done
