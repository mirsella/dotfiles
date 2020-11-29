#!/bin/sh
echo "got : ${@}" >> /home/mirsella/wifidriverinstall.txt

cd /home/mirsella/Documents/RTL88x2BU-Linux-Driver
before=$(git rev-parse HEAD)
git pull
[ $(git rev-parse HEAD) != $before ] && { make clean; make; }
sudo make install
