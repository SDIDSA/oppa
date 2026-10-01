#!/bin/bash
export AT_SPI_BUS_ADDRESS="unix:path=/run/user/1000/at-spi/bus_0"
gdbus introspect --address "$AT_SPI_BUS_ADDRESS" --dest org.a11y.atspi.Registry --object-path /org/a11y/atspi/accessible/root 2>&1 | head -100
