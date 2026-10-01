#!/bin/bash
cat /tmp/oppa-a11y-addr.txt 2>/dev/null
echo "--- procs ---"
ps aux 2>/dev/null | grep -E "at-spi|dbus-daemon" | grep -v grep | head -6
