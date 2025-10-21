#!/bin/bash
# RPM pre-uninstall script for tcgui-backend

echo "Stopping tcgui-backend service..."

# Stop the service if it's running
if [ -d /run/systemd/system ]; then
    systemctl stop tcgui-backend.service >/dev/null 2>&1 || true
fi

exit 0