#!/bin/bash
# Orchestrator: a11y bus address -> app (background) -> client -> report.
export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/1000/at-spi/bus_0"
export AT_SPI_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"
JSON=/mnt/c/Users/zinou/Desktop/oppa/crates/oppa-android-app/device-out/atspi_tree.json
pkill -f atspi_bus_app.py 2>/dev/null
rm -f /tmp/oppa-atspi-bus-result.txt
python3 /mnt/c/Users/zinou/Desktop/oppa/spike/atspi_bus_app.py "$JSON" > /tmp/oppa-atspi-app.log 2>&1 &
APP_PID=$!
sleep 2
python3 /mnt/c/Users/zinou/Desktop/oppa/spike/atspi_bus_client.py "$JSON" > /tmp/oppa-atspi-client.log 2>&1
CLIENT_RC=$?
tail -30 /tmp/oppa-atspi-client.log
kill $APP_PID 2>/dev/null
echo "CLIENT_RC=$CLIENT_RC"
cat /tmp/oppa-atspi-bus-result.txt 2>/dev/null
