#!/bin/sh
set -eu

data_dir="${ANY_WATCH_DATA_DIR:-/data}"

if [ "$(id -u)" -eq 0 ]; then
  mkdir -p "$data_dir"
  chown -R any-watch:any-watch "$data_dir"
  exec gosu any-watch "$@"
fi

exec "$@"
