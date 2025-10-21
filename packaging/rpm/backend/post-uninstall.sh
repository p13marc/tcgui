#!/bin/bash
# RPM post-uninstall script for tcgui-backend

echo "Cleaning up tcgui-backend..."

# Disable service and reload systemd
if [ -d /run/systemd/system ]; then
    systemctl disable tcgui-backend.service >/dev/null 2>&1 || true
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

# Remove working directory (only if empty)
if [ -d "/var/lib/tcgui" ]; then
    rmdir /var/lib/tcgui 2>/dev/null || true
fi

echo "tcgui-backend removed successfully"

exit 0