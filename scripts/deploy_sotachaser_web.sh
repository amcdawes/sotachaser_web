#!/usr/bin/env bash
set -euo pipefail

# Configuration (edit if needed)
REMOTE_HOST="dawes@5.78.83.122"
REMOTE_APP_DIR="/opt/sotachaser_web"
REMOTE_WEB_ROOT="/var/www/sotachaser-web"
SSH_KEY="/home/dawes/.ssh/pedal_plotter_hetzner"

usage() {
	cat <<EOF
Usage: $0 [--install-server | --remote-deploy]

Default: build locally and rsync dist/ to the server (local build + push).
--install-server : Install remote deploy helper on the server (creates $REMOTE_APP_DIR, clones repo, writes /usr/local/bin/deploy_sotachaser).
--remote-deploy  : Run the remote deploy helper on the server (build on server and sync to web root).
EOF
}

if [[ ${1-} == "--install-server" ]]; then
	echo "Installing remote deploy helper on $REMOTE_HOST..."

	ssh -i "$SSH_KEY" "$REMOTE_HOST" bash -s <<REMOTE
set -euo pipefail
sudo mkdir -p "${REMOTE_APP_DIR}"
if [ ! -d "${REMOTE_APP_DIR}/.git" ]; then
	sudo rm -rf "${REMOTE_APP_DIR}"/* || true
	sudo mkdir -p "${REMOTE_APP_DIR}"
	sudo chown -R $(whoami):$(whoami) "${REMOTE_APP_DIR}"
	git clone https://github.com/amcdawes/sotachaser_web.git "${REMOTE_APP_DIR}"
else
	sudo chown -R $(whoami):$(whoami) "${REMOTE_APP_DIR}"
	(cd "${REMOTE_APP_DIR}" && git pull)
fi

cat > /tmp/deploy_sotachaser_remote.sh <<SCRIPT
#!/usr/bin/env bash
set -euo pipefail
cd "${REMOTE_APP_DIR}"
if [ ! -d .git ]; then
	git clone https://github.com/amcdawes/sotachaser_web.git .
else
	git pull
fi
# Build on server (assumes Rust + trunk are installed)
trunk build --release
# Sync built site to web root (requires sudo to write web root)
sudo rsync -avz --delete dist/ "${REMOTE_WEB_ROOT}/"
echo "Remote deploy complete: https://sotachaser.daweslab.com"
SCRIPT

sudo mv /tmp/deploy_sotachaser_remote.sh /usr/local/bin/deploy_sotachaser
sudo chmod +x /usr/local/bin/deploy_sotachaser
echo "Installed /usr/local/bin/deploy_sotachaser on remote host."
REMOTE

	echo "Remote helper installed. You can run it via: ssh -i $SSH_KEY $REMOTE_HOST sudo /usr/local/bin/deploy_sotachaser"
	exit 0
fi

if [[ ${1-} == "--remote-deploy" ]]; then
	echo "Triggering remote deploy on $REMOTE_HOST..."
	ssh -i "$SSH_KEY" "$REMOTE_HOST" "sudo /usr/local/bin/deploy_sotachaser"
	exit 0
fi

if [[ ${1-} == "--help" || ${1-} == "-h" ]]; then
	usage
	exit 0
fi

# Default behavior: build locally then push
echo "Building locally with trunk..."
trunk build --release

echo "Pushing dist/ to $REMOTE_HOST:$REMOTE_WEB_ROOT (using sudo on remote)"
rsync -avz --delete -e "ssh -i $SSH_KEY" --rsync-path="sudo rsync" dist/ "$REMOTE_HOST:$REMOTE_WEB_ROOT/"

echo "Deploy complete: https://sotachaser.daweslab.com"
