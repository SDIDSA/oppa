#!/bin/bash
# Isolated bus + real registry. NOTE: no `--session` flag on the
# daemon — `--session` overrides --address with the default socket
# and splits the bus (first-attempt failure, recorded).
export DBUS_SESSION_BUS_ADDRESS="unix:path=/tmp/oppa-atspi-bus"
pkill -f at-spi2-registryd 2>/dev/null
rm -f /tmp/oppa-atspi-bus
dbus-daemon --fork --print-address=1 --print-pid=1 --address="$DBUS_SESSION_BUS_ADDRESS"
/usr/libexec/at-spi2-registryd &
sleep 2
gdbus introspect --session --dest org.a11y.atspi.Registry --object-path /org/a11y/atspi/registry 2>&1 | head -80
