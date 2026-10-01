#!/bin/bash
python3 -c 'import gi; print("GI_OK")' 2>&1 | head -1
python3 -c 'import dbus; print("DBUS_OK")' 2>&1 | head -1
