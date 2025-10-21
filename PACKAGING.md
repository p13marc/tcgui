# TC GUI Packaging System

This document describes the comprehensive packaging system for the TC GUI project, enabling easy distribution via DEB and RPM packages with proper system integration, security, and privilege separation.

## Overview

The TC GUI packaging system creates separate, well-integrated system packages for:

- **`tcgui-backend`** - Privileged backend service with systemd integration
- **`tcgui-frontend`** - GUI application with desktop integration

### Package Features

- ✅ **Separate packages** for frontend and backend components
- ✅ **DEB and RPM support** for broad Linux distribution compatibility
- ✅ **Systemd integration** for backend service management
- ✅ **Security configuration** with sudoers rules and capabilities
- ✅ **Desktop integration** for GUI application
- ✅ **Proper dependencies** and system requirements
- ✅ **Installation/removal scripts** for clean setup and teardown
- ✅ **Package validation** and testing automation

## Architecture

### tcgui-backend Package

**Purpose**: Privileged network operations service

**Key Components**:
- Binary: `/usr/bin/tcgui-backend`
- Service: `/etc/systemd/system/tcgui-backend.service`
- Sudoers: `/etc/sudoers.d/tcgui-backend`
- Working directory: `/var/lib/tcgui/`

**Dependencies**:
- **DEB**: `iproute2`, `sudo`
- **RPM**: `iproute`, `sudo`, `systemd`

**Installation behavior**:
1. Creates system directories with proper permissions
2. Validates sudoers file syntax
3. Enables systemd service (but doesn't start it)
4. Provides clear instructions for starting the service

### tcgui-frontend Package

**Purpose**: GUI application for traffic control management

**Key Components**:
- Binary: `/usr/bin/tcgui-frontend`
- Desktop entry: `/usr/share/applications/tcgui.desktop`
- Icon: `/usr/share/pixmaps/tcgui.png`

**Dependencies**:
- **DEB**: `libssl3`, `ca-certificates`
- **RPM**: `openssl`, `ca-certificates`

**Installation behavior**:
1. Updates desktop application database
2. Updates icon cache
3. Makes application available in desktop menus

## Getting Started

### 1. Install Packaging Tools

```bash
# Install required packaging tools
just setup-packaging-tools

# This installs:
# - cargo-deb (for DEB packages)
# - cargo-generate-rpm (for RPM packages)
```

### 2. Generate Packages

```bash
# Generate all packages (DEB + RPM for both components)
just package

# Generate specific formats
just package-deb          # DEB packages only
just package-rpm          # RPM packages only

# Generate specific components
just package-backend      # Backend packages only
just package-frontend     # Frontend packages only

# Custom format for specific component
just package-backend deb  # Backend DEB package only
just package-frontend rpm # Frontend RPM package only
```

### 3. List and Validate Packages

```bash
# List all generated packages
just list-packages

# Validate package structure and metadata
just validate-packages

# Test package installation (requires sudo)
just test-packages
```

## Package Contents

### Backend Package File Layout

```
/usr/bin/tcgui-backend                    # Main binary
/etc/systemd/system/tcgui-backend.service # Systemd service file
/etc/sudoers.d/tcgui-backend             # Sudo configuration
/var/lib/tcgui/                          # Working directory
/usr/share/doc/tcgui-backend/README.md   # Documentation
```

### Frontend Package File Layout

```
/usr/bin/tcgui-frontend                  # Main binary
/usr/share/applications/tcgui.desktop    # Desktop entry
/usr/share/pixmaps/tcgui.png            # Application icon
/usr/share/doc/tcgui-frontend/README.md # Documentation
```

## System Integration

### Systemd Service Configuration

The backend package includes a properly configured systemd service:

**Key Features**:
- Runs as root with network capabilities
- Security hardening with restricted permissions
- Automatic restart on failure
- Proper dependency ordering
- Environment variable configuration
- Structured logging to systemd journal

**Service Management**:
```bash
# Enable and start service
sudo systemctl enable tcgui-backend
sudo systemctl start tcgui-backend

# Check status
sudo systemctl status tcgui-backend

# View logs
sudo journalctl -u tcgui-backend -f
```

### Security Configuration

#### Sudoers Rules

The backend package includes restricted sudoers rules allowing only specific network commands:

```bash
# Allowed commands:
/usr/sbin/tc qdisc add *
/usr/sbin/tc qdisc del *
/usr/sbin/tc qdisc replace *
/usr/sbin/ip netns exec * /usr/sbin/tc qdisc *
/usr/sbin/ip -json netns list
/usr/sbin/ip -json link show
```

#### Systemd Security

The service includes comprehensive security settings:
- Capability restrictions (`CAP_NET_ADMIN`, `CAP_NET_RAW`)
- Private temporary directories
- Protected system directories
- Memory execution protection
- Namespace restrictions as needed

### Desktop Integration

The frontend package provides complete desktop integration:

**Features**:
- Application menu entry with proper categorization
- Standard application icon (64x64 PNG)
- MIME type associations (if needed)
- Desktop database integration
- Icon cache updates

**Launch Methods**:
```bash
# From command line
tcgui-frontend

# From desktop environment
# Available in Applications → Network → TC GUI
```

## Package Testing

### Automated Testing

```bash
# Test all packages on current system
sudo ./scripts/test-packages.sh test-all

# Test specific package format
sudo ./scripts/test-packages.sh test-deb
sudo ./scripts/test-packages.sh test-rpm
```

### Test Coverage

The testing system validates:

1. **Package Installation**: Successful installation via package manager
2. **File Deployment**: All expected files are in correct locations
3. **Binary Execution**: Applications can execute and show help
4. **System Integration**: Services can be enabled, desktop files are valid
5. **Package Removal**: Clean removal without leaving artifacts
6. **Dependency Resolution**: Package manager handles dependencies correctly

### Test Environment

The test system:
- ✅ **Backs up existing files** before testing
- ✅ **Restores original state** after testing
- ✅ **Detects package manager** automatically (DEB/RPM)
- ✅ **Validates package metadata** and structure
- ✅ **Tests installation/removal cycle**
- ✅ **Verifies system integration**

## Distribution Support

### Debian/Ubuntu (DEB packages)

**Tested on**:
- Debian 11+ (Bullseye)
- Ubuntu 20.04+ (Focal)

**Installation**:
```bash
# Install both packages
sudo dpkg -i tcgui-backend_*.deb tcgui-frontend_*.deb
sudo apt-get install -f  # Fix any missing dependencies

# Or using apt (if in repository)
sudo apt install tcgui-backend tcgui-frontend
```

### Fedora/RHEL/CentOS (RPM packages)

**Tested on**:
- Fedora 35+ 
- RHEL/CentOS 8+
- Rocky Linux 8+

**Installation**:
```bash
# Install both packages
sudo rpm -i tcgui-backend-*.rpm tcgui-frontend-*.rpm

# Or using dnf
sudo dnf install tcgui-backend tcgui-frontend
```

## Package Maintenance

### Version Updates

To create new package versions:

1. **Update version in Cargo.toml files**:
   ```toml
   [package]
   version = "0.2.0"  # Update this
   ```

2. **Update changelog** (for DEB packages):
   ```bash
   # Edit debian/changelog with new version
   tcgui (0.2.0-1) unstable; urgency=medium
     * New features and bug fixes
    -- Your Name <you@example.com>  Wed, 01 Jan 2024 12:00:00 +0000
   ```

3. **Regenerate packages**:
   ```bash
   just package
   ```

### Configuration Updates

**System Service Configuration**:
- Edit `packaging/systemd/tcgui-backend.service`
- Rebuild and test packages

**Security Configuration**:
- Edit `packaging/sudoers/tcgui-backend`
- Ensure syntax is valid with `visudo -c`

**Desktop Integration**:
- Edit `packaging/desktop/tcgui.desktop`
- Update application icon at `packaging/icons/tcgui.png`

## Advanced Usage

### Custom Package Configuration

You can customize package metadata by editing the `[package.metadata.deb]` and `[package.metadata.generate-rpm]` sections in the respective Cargo.toml files.

**Example customizations**:
- Dependencies and conflicts
- Pre/post installation scripts
- File permissions and ownership
- Package descriptions and categories

### Cross-Distribution Packaging

```bash
# Create packages for specific distributions
./scripts/package.sh backend deb  # DEB backend only
./scripts/package.sh frontend rpm # RPM frontend only

# Custom package naming and versioning
# Edit Cargo.toml metadata sections before packaging
```

### Package Signing

For production distribution, consider signing packages:

**DEB packages**:
```bash
# Sign with debsigs
debsigs --sign=origin tcgui-backend_*.deb
```

**RPM packages**:
```bash
# Sign with rpm --addsign
rpm --addsign tcgui-backend-*.rpm
```

## Troubleshooting

### Common Issues

1. **Missing dependencies during build**:
   ```bash
   # Install packaging tools
   just setup-packaging-tools
   ```

2. **Package validation fails**:
   ```bash
   # Check package structure
   just validate-packages
   
   # For DEB packages
   dpkg-deb --info package.deb
   
   # For RPM packages  
   rpm -qip package.rpm
   ```

3. **Service fails to start**:
   ```bash
   # Check service logs
   sudo journalctl -u tcgui-backend -f
   
   # Verify sudoers file
   sudo visudo -c -f /etc/sudoers.d/tcgui-backend
   ```

4. **Desktop integration issues**:
   ```bash
   # Update desktop database
   sudo update-desktop-database /usr/share/applications
   
   # Validate desktop file
   desktop-file-validate /usr/share/applications/tcgui.desktop
   ```

### Package Debugging

```bash
# List package contents
dpkg-deb -c package.deb  # DEB packages
rpm -qlp package.rpm     # RPM packages

# Extract package contents
dpkg-deb -x package.deb /tmp/extract/  # DEB packages
rpm2cpio package.rpm | cpio -idmv     # RPM packages

# Check package scripts
dpkg-deb -e package.deb /tmp/scripts/  # DEB package scripts
rpm -qp --scripts package.rpm         # RPM package scripts
```

## Security Considerations

### Privilege Requirements

**Backend Package**:
- Requires root installation (system service)
- Uses restricted sudoers rules for network operations
- Runs with minimal required capabilities
- Uses systemd security features for isolation

**Frontend Package**:
- Can be installed by regular users (user-space application)
- No special privileges required for operation
- Communicates with backend via standard protocols

### Network Security

- Backend service listens only on localhost by default
- Communication uses Zenoh pub-sub with authentication
- Traffic control operations are limited by sudoers rules
- No network ports exposed externally

## Future Enhancements

Planned improvements to the packaging system:

1. **Repository Integration**: APT and YUM repository setup
2. **Snap/Flatpak Support**: Universal Linux packaging
3. **Container Images**: Docker containers for deployment
4. **Configuration Management**: Ansible/Puppet modules
5. **Monitoring Integration**: Prometheus metrics packaging
6. **Multi-Architecture Support**: ARM64, x86 packages

## Conclusion

The TC GUI packaging system provides production-ready, secure, and well-integrated packages for Linux distributions. With comprehensive testing, proper security configuration, and extensive documentation, it enables easy deployment and maintenance of the TC GUI application in enterprise and personal environments.

The system follows Linux packaging best practices while maintaining the security and privilege separation requirements of the underlying application architecture.