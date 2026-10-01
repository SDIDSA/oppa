#!/bin/bash
export AT_SPI_BUS_ADDRESS="unix:path=/run/user/1000/at-spi/bus_0"
export DBUS_SESSION_BUS_ADDRESS="$AT_SPI_BUS_ADDRESS"
AT_SPI_BUS_ADDRESS="$AT_SPI_BUS_ADDRESS" /usr/libexec/at-spi2-registryd &
sleep 2
gdbus introspect --address "$AT_SPI_BUS_ADDRESS" --dest org.a11y.atspi.Registry --object-path /org/a11y/atspi/registry 2>&1 | head -70
