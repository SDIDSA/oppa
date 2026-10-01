#!/bin/bash
ls /usr/include/at-spi-2.0/atspi/ 2>/dev/null || echo "NO_HEADERS"
which gcc cc 2>/dev/null || echo "NO_CC"
