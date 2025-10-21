#!/bin/bash
# RPM post-install script for tcgui-backend

echo "Configuring tcgui-backend..."

# Create working directory
if [ ! -d "/var/lib/tcgui" ]; then
    mkdir -p /var/lib/tcgui
    chown root:root /var/lib/tcgui
    chmod 755 /var/lib/tcgui
fi

# Validate sudoers file syntax
if [ -f "/etc/sudoers.d/tcgui-backend" ]; then
    if ! visudo -c -f /etc/sudoers.d/tcgui-backend; then
        echo "ERROR: Invalid sudoers syntax in /etc/sudoers.d/tcgui-backend"
        echo "Please check the file and fix any syntax errors"
        exit 1
    fi
fi

# Reload systemd and enable service
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
    systemctl enable tcgui-backend.service >/dev/null 2>&1 || true
    echo "tcgui-backend service enabled (use 'systemctl start tcgui-backend' to start)"
fi

echo "tcgui-backend installation completed successfully"
echo ""
echo "To start the service:"
echo "  sudo systemctl start tcgui-backend"
echo ""
echo "To check service status:"
echo "  sudo systemctl status tcgui-backend"
echo ""
echo "To view logs:"
echo "  sudo journalctl -u tcgui-backend -f"

exit 0