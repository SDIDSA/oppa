#!/bin/bash
B=/usr/lib/x86_64-linux-gnu/libatspi.so.0.0.1
for s in entry selectable text link window label menu; do
  echo "== $s =="
  strings "$B" | grep -x -E ".{0,4}$s.{0,4}" | head -8
done
