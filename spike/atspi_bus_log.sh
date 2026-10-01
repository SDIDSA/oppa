#!/bin/bash
cat /tmp/oppa-atspi-app.log 2>/dev/null | head -25
echo "--- app procs ---"
ps aux 2>/dev/null | grep atspi_bus_app | grep -v grep | head -3
