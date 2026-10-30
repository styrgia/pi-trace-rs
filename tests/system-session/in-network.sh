#!/bin/sh
set -eu
ip link set lo up
exec "$@"
