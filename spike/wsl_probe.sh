#!/bin/bash
python3 -c 'import dbus; print("PYDBUS_OK")' 2>&1 | head -1
which gdbus busctl qdbus dbus-send dbus-monitor 2>/dev/null
python3 --version
ls /usr/lib/python3/dist-packages/ 2>/dev/null | head -20
