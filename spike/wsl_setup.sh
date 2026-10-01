#!/bin/bash
set -e
python3 -m ensurepip --user 2>&1 | tail -1
python3 -m pip install --user dasbus 2>&1 | tail -2
python3 -c 'import dasbus; print("DASBUS_OK")'
