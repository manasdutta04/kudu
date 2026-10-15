#!/bin/sh
# After installing Kudu: load the virtual keyboard module and apply its udev rule now, so Kudu
# can type without a reboot. Adding the user to the input group is left to them (Kudu shows
# the command), because a package can't know which user will run it.
modprobe uinput 2>/dev/null || true
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules 2>/dev/null || true
  udevadm trigger --subsystem-match=misc 2>/dev/null || true
fi
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
echo "Kudu is installed. If you're not in the 'input' group yet, run: sudo usermod -aG input \$USER (then log out and in)."
exit 0
