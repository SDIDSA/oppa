#!/bin/sh
echo "== toolchains =="
which cargo rustc cmake pkg-config Xvfb weston
echo "== rust =="
rustc --version
cargo --version
echo "== display libs =="
ldconfig -p > /tmp/ld.txt 2>/dev/null
grep -i "libX11\." /tmp/ld.txt | head -3
grep -i "libwayland-client" /tmp/ld.txt | head -3
grep -i "libxkbcommon" /tmp/ld.txt | head -3
grep -i "libEGL" /tmp/ld.txt | head -3
grep -i "harfbuzz" /tmp/ld.txt | head -3
grep -i "fontconfig" /tmp/ld.txt | head -3
echo "== dev headers =="
ls /usr/include/X11/Xlib.h 2>&1
ls /usr/include/wayland-client.h 2>&1
echo "== sudo =="
sudo -n true
echo "sudo_exit=$?"
echo "== mesa/vulkan =="
ls /usr/share/vulkan/icd.d/ 2>&1
which vulkaninfo glxinfo 2>&1
