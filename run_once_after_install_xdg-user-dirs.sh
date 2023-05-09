#!/bin/bash

if ! hostnamectl hostname | grep 42paris; then
	sudo cp ./etc/xdg/user-dirs.defaults /etc/xdg
fi
