#!/bin/sh
# Runs a check and, if it fails, puts its last lines in an annotation on
# GitHub, so the reason is visible on the run's page without opening
# the full log.
#   ci/run.sh "Title" command args…
title=$1
shift
log=$(mktemp)
"$@" >"$log" 2>&1
code=$?
cat "$log"
if [ "$code" != 0 ]; then
    body=$(grep -v -e 'portal' -e 'a11y' -e 'dbus-daemon' "$log" | tail -n 30 | cut -c1-200 |
        sed -e 's/%/%25/g' -e 's/\r//g' | awk 'BEGIN{ORS="%0A"} {print}')
    echo "::error title=$title (exit $code)::$body"
fi
exit "$code"
