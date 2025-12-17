# Custom Preset Format Specification

Custom presets allow you to define reusable network condition configurations that can be applied via the UI or referenced in scenarios.

## File Format

Presets use JSON5 format, which extends JSON with:
- Comments (`//` and `/* */`)
- Trailing commas
- Unquoted keys
- Human-readable syntax

## File Locations

Presets are loaded from the following directories (in priority order, later overrides earlier):

1. **System**: `/usr/share/tcgui/presets` - Package-installed presets
2. **User**: `~/.config/tcgui/presets` - User-defined presets
3. **Local**: `./presets` - Project-local presets

Files must have the `.json5` extension.

## Schema

```json5
{
    // Required: Unique identifier (used for references in scenarios)
    id: "preset-id",
    
    // Required: Human-readable name displayed in the UI
    name: "Preset Display Name",
    
    // Optional: Detailed description
    description: "What network conditions this preset simulates",
    
    // TC configuration fields - all optional
    // Presence of a field automatically enables that feature
    
    loss: {
        percentage: 5.0,      // 0.0-100.0: Packet loss percentage
        correlation: 25.0,    // 0.0-100.0: Correlation with previous packet
    },
    
    delay: {
        base_ms: 100.0,       // 0.0-5000.0: Base delay in milliseconds
        jitter_ms: 20.0,      // 0.0-1000.0: Delay variation
        correlation: 25.0,    // 0.0-100.0: Jitter correlation
    },
    
    duplicate: {
        percentage: 1.0,      // 0.0-100.0: Duplication percentage
        correlation: 0.0,     // 0.0-100.0: Correlation
    },
    
    reorder: {
        percentage: 5.0,      // 0.0-100.0: Reorder percentage
        correlation: 0.0,     // 0.0-100.0: Correlation
        gap: 5,               // 1-10: Gap parameter
    },
    
    corrupt: {
        percentage: 0.1,      // 0.0-100.0: Corruption percentage
        correlation: 0.0,     // 0.0-100.0: Correlation
    },
    
    rate_limit: {
        rate_kbps: 1000,      // 1-1000000: Rate limit in kbps
    },
}
```

## Examples

### Office VPN Connection

```json5
// ~/.config/tcgui/presets/office-vpn.json5
{
    id: "office-vpn",
    name: "Office VPN",
    description: "Typical VPN connection to corporate office",
    
    delay: {
        base_ms: 45,
        jitter_ms: 8,
    },
    loss: {
        percentage: 0.3,
    },
}
```

### Satellite Internet

```json5
// ~/.config/tcgui/presets/satellite-internet.json5
{
    id: "satellite-internet",
    name: "Satellite Internet",
    description: "High latency satellite broadband connection",
    
    delay: {
        base_ms: 600,
        jitter_ms: 30,
        correlation: 25,
    },
    loss: {
        percentage: 2.0,
        correlation: 15,
    },
    rate_limit: {
        rate_kbps: 1500,
    },
}
```

### Network Stress Test

```json5
// ./presets/stress-test.json5
{
    id: "stress-test",
    name: "Network Stress Test",
    description: "Extreme conditions for stress testing applications",
    
    loss: {
        percentage: 20,
    },
    delay: {
        base_ms: 500,
        jitter_ms: 200,
    },
    duplicate: {
        percentage: 5,
    },
    corrupt: {
        percentage: 2,
    },
    rate_limit: {
        rate_kbps: 256,
    },
}
```

### 3G Mobile Network

```json5
{
    id: "3g-mobile",
    name: "3G Mobile Network",
    description: "Legacy 3G mobile connection with variable quality",
    
    delay: {
        base_ms: 150,
        jitter_ms: 80,
        correlation: 30,
    },
    loss: {
        percentage: 3,
        correlation: 20,
    },
    rate_limit: {
        rate_kbps: 1500,
    },
}
```

### Minimal Preset

Only include the features you need:

```json5
{
    id: "high-latency",
    name: "High Latency Only",
    description: "Just adds 200ms latency, no other effects",
    
    delay: {
        base_ms: 200,
    },
}
```

## Using Custom Presets

### In the UI

Custom presets appear in the preset dropdown alongside built-in presets. They are displayed under a "Custom" section.

### In Scenarios

Reference presets by their `id` in scenario steps:

```json5
{
    id: "vpn-test",
    name: "VPN Test Scenario",
    steps: [
        {
            duration: "1m",
            description: "Normal VPN",
            preset: "office-vpn"  // References custom preset
        },
        {
            duration: "30s",
            description: "Stress test",
            preset: "stress-test"  // References another custom preset
        },
    ],
}
```

## Built-in Presets

TC GUI includes several built-in presets that are always available:

| Preset | Description |
|--------|-------------|
| Satellite Link | 500ms delay, 1% loss, 2 Mbps |
| Cellular Network | 150ms delay with jitter, 2% loss, reordering |
| Poor WiFi | 80ms delay, 8% loss, corruption |
| WAN Link | 50ms delay, 0.5% loss, 50 Mbps |
| Unreliable Connection | 200ms delay, 15% loss, duplication, reordering |
| High Latency + Low Bandwidth | 800ms delay, 3% loss, 512 kbps |
| Test All | All features enabled for testing |

Built-in presets can be referenced in scenarios using IDs like `satellite-link`, `cellular-network`, `poor-wifi`, etc.

## Validation

Presets are validated when loaded:
- `id` and `name` must be non-empty
- TC parameter values must be within valid ranges
- JSON5 syntax must be valid

Invalid presets are skipped during loading with a warning logged.

## Priority and Overriding

When multiple presets share the same `id`:
- Presets in later directories override earlier ones
- Local (`./presets`) overrides User (`~/.config/tcgui/presets`)
- User overrides System (`/usr/share/tcgui/presets`)

This allows customizing or replacing built-in presets without modifying system files.

## CLI Configuration

The backend supports CLI options to customize preset loading:

### `--preset-dir <DIR>`

Add additional preset directories. Can be specified multiple times. Additional directories have higher priority than default directories.

```bash
# Add a team-shared preset directory
tcgui-backend --preset-dir /team/shared/presets

# Add multiple directories
tcgui-backend --preset-dir /team/presets --preset-dir /project/presets
```

### `--no-default-presets`

Skip loading presets from default directories. Only presets from directories specified with `--preset-dir` will be loaded.

```bash
# Use only custom presets
tcgui-backend --no-default-presets --preset-dir /my/presets

# Useful for isolated testing environments
tcgui-backend --no-default-presets --preset-dir ./test-presets
```

### Examples

```bash
# Default behavior: load from system, user, and local directories
tcgui-backend

# Add extra directories on top of defaults
tcgui-backend --preset-dir /opt/custom-presets

# Use only specific directories (ignore defaults)
tcgui-backend --no-default-presets \
    --preset-dir /etc/tcgui/presets \
    --preset-dir /home/user/presets
```
