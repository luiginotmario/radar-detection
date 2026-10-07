#!/usr/bin/env bash
# builds for the pi and installs it there over ssh. run it again to update.
#
#   ./scripts/deploy.sh pi@radar.local
#   ./scripts/deploy.sh pi@radar.local --lan     also open the radar from your phone
set -euo pipefail
HOST="${1:?usage: ./scripts/deploy.sh user@host [--lan]}"
shift
cd "$(dirname "$0")/.."

./scripts/build-pi.sh

echo "→ copying to $HOST"
ssh "$HOST" 'mkdir -p ~/radar-detection'
scp -q build/pi/radar-detection build/pi/setup.sh "$HOST:radar-detection/"

echo "→ setting up"
ssh -t "$HOST" "bash ~/radar-detection/setup.sh $*"
