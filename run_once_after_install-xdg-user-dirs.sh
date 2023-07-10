#!/bin/bash

if ! hostnamectl hostname | grep 42paris; then
	sudo cp "$(chezmoi source-path)"/etc/xdg/user-dirs.defaults /etc/xdg
fi
