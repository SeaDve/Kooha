#!/bin/sh

# Forked from https://gitlab.gnome.org/World/lollypop/-/blob/master/bin/revision.sh

VERSION="@VERSION@"
if [[ $VERSION != "@VERSION@" ]]
then
	echo $VERSION
else
	git describe --tags | sed 's/\([^-]*-g\)/r\1/;s/-/./g'
fi
