# Scenario File Format Specification

TC GUI uses JSON5 format for scenario files, providing a human-friendly syntax with comments and trailing commas.

## File Location

Scenarios are loaded from these directories (in priority order, later overrides earlier):

1. `/usr/share/tcgui/scenarios` - System-wide scenarios
2. `~/.config/tcgui/scenarios` - User scenarios
3. `./scenarios` - Local project scenarios

Use `--no-default-scenarios` flag to disable automatic loading from these directories.

## Basic Structure

```json5
{
    id: "unique-scenario-id",
    name: "Human Readable Name",
    description: "Detailed description of what this scenario simulates",
    
    // Optional: repeat scenario indefinitely
    loop_scenario: false,
    
    // Optional: restore TC config on failure (default: true)
    cleanup_on_failure: true,
    
    // Optional metadata
    metadata: {
        tags: ["tag1", "tag2"],
        author: "Your Name",
        version: "1.0",
    },
    
    // Required: list of steps
    steps: [
        // ... steps here
    ],
}
```

## Field Reference

### Top-Level Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `id` | string | Yes | - | Unique identifier (alphanumeric, hyphens allowed) |
| `name` | string | Yes | - | Display name in the UI |
| `description` | string | No | `""` | Detailed description |
| `loop_scenario` | boolean | No | `false` | Repeat indefinitely when complete |
| `cleanup_on_failure` | boolean | No | `true` | Restore original TC config on failure/abort |
| `metadata` | object | No | `{}` | Additional metadata |
| `steps` | array | Yes | - | List of scenario steps (min: 1) |

### Metadata Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `tags` | string[] | No | `[]` | Searchable tags |
| `author` | string | No | `null` | Scenario author |
| `version` | string | No | `"1.0"` | Scenario version |

### Step Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `duration` | string | Yes | - | How long to apply this step |
| `description` | string | Yes | - | Step description (shown in UI) |
| `tc_config` | object | No | `{}` | Traffic control configuration |

## Duration Format

Durations use human-readable strings:

| Format | Example | Milliseconds |
|--------|---------|--------------|
| Milliseconds | `"500ms"` | 500 |
| Seconds | `"30s"` | 30,000 |
| Minutes | `"5m"` | 300,000 |
| Hours | `"1h"` | 3,600,000 |
| Combined | `"1m30s"` | 90,000 |
| Combined | `"1h30m"` | 5,400,000 |

## TC Configuration

The `tc_config` object controls Linux Traffic Control (tc netem) settings. **Fields are implicitly enabled when present** - no need for `enabled: true`.

### Available Features

#### Loss Configuration

```json5
tc_config: {
    loss: {
        percentage: 5.0,      // 0.0-100.0: packet loss percentage
        correlation: 25.0,    // 0.0-100.0: correlation with previous loss
    },
}
```

#### Delay Configuration

```json5
tc_config: {
    delay: {
        base_ms: 100.0,       // 0.0-5000.0: base delay in milliseconds
        jitter_ms: 20.0,      // 0.0-1000.0: random jitter (+/-)
        correlation: 25.0,    // 0.0-100.0: correlation with previous delay
    },
}
```

#### Duplicate Configuration

```json5
tc_config: {
    duplicate: {
        percentage: 1.0,      // 0.0-100.0: packet duplication percentage
        correlation: 0.0,     // 0.0-100.0: correlation
    },
}
```

#### Reorder Configuration

```json5
tc_config: {
    reorder: {
        percentage: 5.0,      // 0.0-100.0: reorder percentage
        correlation: 0.0,     // 0.0-100.0: correlation
        gap: 5,               // 1-10: reorder gap (packets)
    },
}
```

#### Corrupt Configuration

```json5
tc_config: {
    corrupt: {
        percentage: 0.1,      // 0.0-100.0: corruption percentage
        correlation: 0.0,     // 0.0-100.0: correlation
    },
}
```

#### Rate Limit Configuration

```json5
tc_config: {
    rate_limit: {
        rate_kbps: 1000,      // 1-1000000: rate limit in kbps
    },
}
```

### Combining Features

Multiple features can be combined in a single step:

```json5
tc_config: {
    loss: { percentage: 5 },
    delay: { base_ms: 100, jitter_ms: 20 },
    rate_limit: { rate_kbps: 5000 },
}
```

### Clearing All Settings

Use an empty `tc_config` to disable all impairments:

```json5
{
    duration: "30s",
    description: "Clean network - no impairments",
    tc_config: {},
}
```

## Parameter Ranges

| Parameter | Min | Max | Unit |
|-----------|-----|-----|------|
| Loss percentage | 0.0 | 100.0 | % |
| Loss correlation | 0.0 | 100.0 | % |
| Delay base | 0.0 | 5000.0 | ms |
| Delay jitter | 0.0 | 1000.0 | ms |
| Delay correlation | 0.0 | 100.0 | % |
| Duplicate percentage | 0.0 | 100.0 | % |
| Duplicate correlation | 0.0 | 100.0 | % |
| Reorder percentage | 0.0 | 100.0 | % |
| Reorder correlation | 0.0 | 100.0 | % |
| Reorder gap | 1 | 10 | packets |
| Corrupt percentage | 0.0 | 100.0 | % |
| Corrupt correlation | 0.0 | 100.0 | % |
| Rate limit | 1 | 1,000,000 | kbps |

## Validation Rules

1. **ID**: Must not be empty
2. **Name**: Must not be empty
3. **Steps**: At least one step required
4. **Step duration**: Must be greater than 0
5. **Step description**: Must not be empty
6. **Total duration**: Cannot exceed 24 hours
7. **Parameters**: Must be within valid ranges

## Complete Example

```json5
// Satellite Link Simulation
// Simulates high-latency satellite communication with variable conditions
{
    id: "satellite-link",
    name: "Satellite Link Simulation",
    description: "Simulate geostationary satellite uplink with ~600ms RTT",
    
    metadata: {
        tags: ["satellite", "high-latency", "wan"],
        author: "Network Team",
        version: "1.0",
    },
    
    // Don't loop - run once
    loop_scenario: false,
    
    // Restore network on failure
    cleanup_on_failure: true,
    
    steps: [
        {
            duration: "1m",
            description: "Clear sky - optimal satellite conditions",
            tc_config: {
                delay: { base_ms: 300, jitter_ms: 10 },
                loss: { percentage: 0.1 },
            },
        },
        {
            duration: "30s",
            description: "Rain fade - signal degradation",
            tc_config: {
                delay: { base_ms: 350, jitter_ms: 50 },
                loss: { percentage: 2, correlation: 30 },
            },
        },
        {
            duration: "30s",
            description: "Heavy rain - significant impairment",
            tc_config: {
                delay: { base_ms: 400, jitter_ms: 100 },
                loss: { percentage: 8, correlation: 40 },
                rate_limit: { rate_kbps: 2000 },
            },
        },
        {
            duration: "1m",
            description: "Recovery - conditions improving",
            tc_config: {
                delay: { base_ms: 320, jitter_ms: 20 },
                loss: { percentage: 0.5 },
            },
        },
    ],
}
```

## JSON5 Features

TC GUI supports full JSON5 syntax:

- **Comments**: `// single line` and `/* multi-line */`
- **Trailing commas**: Allowed in objects and arrays
- **Unquoted keys**: `id:` instead of `"id":`
- **Single quotes**: `'value'` same as `"value"`

## See Also

- [Best Practices](best-practices.md) - Guidelines for effective scenario design
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [Examples](examples.md) - Annotated example scenarios
