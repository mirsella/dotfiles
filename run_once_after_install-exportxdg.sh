#!/bin/bash
if hostnamectl hostname | grep -Eq "main|laptop|raspberrypi|tosh|unowhy"; then
	echo "source $HOME/.config/zsh/exportxdg" | sudo tee /etc/profile.d/exportxdg.sh
fi
