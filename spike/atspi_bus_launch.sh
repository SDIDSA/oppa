#!/bin/bash
# Canonical flow, detached: launcher owns the a11y bus + registry,
# address captured to a file for the app/client scripts.
pkill -f at-spi-bus-launcher 2>/dev/null
pkill -f at-spi2-registryd 2>/dev/null
sleep 1
nohup /usr/libexec/at-spi-bus-launcher --launch-immediately > /tmp/oppa-a11y-addr.txt 2>&1 &
sleep 3
cat /tmp/oppa-a11y-addr.txt
