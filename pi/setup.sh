#!/usr/bin/env bash
# sets up radar-detection on a raspberry pi: it starts when the pi turns on and opens
# full screen on the display. run it from the folder with the radar-detection binary
# (scripts/deploy.sh does this for you). safe to run again, e.g. to update.
#
#   --lan   let phones and laptops on your wifi open the radar too, at http://<pi>:7070
set -euo pipefail
cd "$(dirname "$0")"

LAN=no
for arg in "$@"; do
  case "$arg" in
    --lan) LAN=yes ;;
    *) echo "unknown option: $arg"; exit 1 ;;
  esac
done
ME="$(id -un)"

echo "→ installing radar-detection"
sudo install -m 755 radar-detection /usr/local/bin/radar-detection

# options live here so they survive updates. after editing:
#   sudo systemctl restart radar-detection
if [ ! -f /etc/default/radar-detection ]; then
  sudo tee /etc/default/radar-detection >/dev/null <<'CONF'
# options for radar-detection (see `radar-detection --help`), for example:
#   --lan                  let other devices on your wifi open the radar
#   --ld2450 PORT@X,Y,DEG  place sensors yourself instead of finding them
RADAR_OPTIONS=""
CONF
fi
if [ "$LAN" = yes ] && ! grep -q -- '--lan' /etc/default/radar-detection; then
  sudo sed -i 's/^RADAR_OPTIONS="/RADAR_OPTIONS="--lan /' /etc/default/radar-detection
fi

echo "→ starting it at boot"
sudo tee /etc/systemd/system/radar-detection.service >/dev/null <<UNIT
[Unit]
Description=radar-detection
After=network.target

[Service]
User=$ME
SupplementaryGroups=dialout
EnvironmentFile=/etc/default/radar-detection
ExecStart=/usr/local/bin/radar-detection \$RADAR_OPTIONS
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
UNIT

# plugging a sensor in later makes it look for sensors again (cp210x and ch340 adapters)
sudo tee /etc/udev/rules.d/90-radar-detection.rules >/dev/null <<'RULES'
ACTION=="add", SUBSYSTEM=="tty", ENV{ID_VENDOR_ID}=="10c4", ENV{ID_MODEL_ID}=="ea60", RUN+="/usr/bin/systemctl --no-block restart radar-detection.service"
ACTION=="add", SUBSYSTEM=="tty", ENV{ID_VENDOR_ID}=="1a86", ENV{ID_MODEL_ID}=="7523", RUN+="/usr/bin/systemctl --no-block restart radar-detection.service"
RULES
sudo udevadm control --reload-rules

sudo systemctl daemon-reload
sudo systemctl enable radar-detection.service --quiet
sudo systemctl restart radar-detection.service

echo "→ opening the screen at login"
sudo tee /usr/local/bin/radar-detection-screen >/dev/null <<'SCREEN'
#!/usr/bin/env bash
# waits for the radar to be up, then opens it full screen
for _ in $(seq 1 60); do
  curl -s -o /dev/null http://localhost:7070 && break
  sleep 1
done
BROWSER="$(command -v chromium || command -v chromium-browser)"
exec "$BROWSER" --kiosk --incognito --noerrdialogs --disable-infobars --no-first-run \
  --disable-pinch --overscroll-history-navigation=0 --check-for-update-interval=31536000 \
  --password-store=basic "http://localhost:7070/?kiosk"
SCREEN
sudo chmod 755 /usr/local/bin/radar-detection-screen
mkdir -p ~/.config/autostart
cat > ~/.config/autostart/radar-detection-screen.desktop <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=radar-detection
Exec=/usr/local/bin/radar-detection-screen
DESKTOP

# log straight into the desktop, and keep the display on: it's a radar
if command -v raspi-config >/dev/null; then
  sudo raspi-config nonint do_boot_behaviour B4
  sudo raspi-config nonint do_blanking 1
fi

echo
echo "done. reboot to see it on the display:  sudo reboot"
if grep -q -- '--lan' /etc/default/radar-detection; then
  echo "from your phone: http://$(hostname).local:7070"
fi
echo "logs: journalctl -u radar-detection -f"
