#!/bin/bash
export DBUS_SESSION_BUS_ADDRESS="unix:path=/tmp/oppa-atspi-bus"
pkill -f at-spi2-registryd 2>/dev/null
rm -f /tmp/oppa-atspi-bus
dbus-daemon --session --fork --print-address=1 --print-pid=1 --address="$DBUS_SESSION_BUS_ADDRESS"
sleep 1
echo "--- sockets ---"
ss -x 2>/dev/null | grep -E "oppa|dbus" | head -5
echo "--- names on env bus ---"
dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>&1 | head -8
