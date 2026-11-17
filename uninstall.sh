#!/usr/bin/env bash
set -euo pipefail

echo "=== Desinstalador do Driver Rust + Interface Gráfica Python UGEE ==="

systemctl --user stop ugee-driver.service 2>/dev/null || true
systemctl --user disable ugee-driver.service 2>/dev/null || true
rm -f "${HOME}/.config/systemd/user/ugee-driver.service"
systemctl --user daemon-reload

for sz in 16 32 48 64 128 256 512; do
    rm -f "${HOME}/.local/share/icons/hicolor/${sz}x${sz}/apps/ugee-m708.png"
    sudo rm -f "/usr/share/icons/hicolor/${sz}x${sz}/apps/ugee-m708.png"
done
rm -f "${HOME}/.local/share/pixmaps/ugee-m708.png"
sudo rm -f /usr/share/pixmaps/ugee-m708.png

rm -f "${HOME}/.local/share/applications/ugee-gui.desktop"
sudo rm -f /usr/share/applications/ugee-gui.desktop
gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true
sudo update-desktop-database /usr/share/applications 2>/dev/null || true

sudo rm -f /usr/local/bin/ugee-driver /usr/local/bin/ugee-gui
sudo rm -rf /usr/local/share/ugee-driver
sudo rm -f /etc/udev/rules.d/60-ugee.rules
sudo rm -f /etc/libwacom/ugee-m708.tablet
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw --subsystem-match=usb --subsystem-match=misc || true

echo "Driver e interface gráfica desinstalados com sucesso!"
