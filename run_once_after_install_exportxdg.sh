#!/bin/bash
if ! hostnamectl hostname | grep 42paris; then
	echo 'source "$HOME"/.config/zsh/exportxdg' | sudo tee /etc/profile.d/exportxdg.sh
fi
