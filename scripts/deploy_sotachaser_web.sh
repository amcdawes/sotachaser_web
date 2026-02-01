#!/usr/bin/env bash
set -euo pipefail

REMOTE_HOST="dawes@5.78.83.122"
REMOTE_APP_DIR="/opt/sotachaser_web"
REMOTE_WEB_ROOT="/var/www/sotachaser-web"

# Build locally
trunk build --release

# Sync repo (expects a git clone on server)
ssh -i /home/dawes/.ssh/pedal_plotter_hetzner "$REMOTE_HOST" "cd $REMOTE_APP_DIR && git pull"

# Sync static build to web root
rsync -avz --delete -e "ssh -i /home/dawes/.ssh/pedal_plotter_hetzner" dist/ "$REMOTE_HOST:$REMOTE_WEB_ROOT/"

echo "Deploy complete: https://sotachaser.daweslab.com"
