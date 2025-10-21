#!/bin/bash
# RPM post-install script for tcgui-frontend

echo "Configuring tcgui-frontend..."

# Update desktop database
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi

# Update icon cache
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q /usr/share/pixmaps || true
fi

echo "tcgui-frontend installation completed successfully"
echo ""
echo "The TC GUI application is now available:"
echo "  - Launch from applications menu"
echo "  - Run from terminal: tcgui-frontend"
echo ""
echo "Note: The backend service (tcgui-backend) must be running"
echo "for the frontend to function properly."

exit 0