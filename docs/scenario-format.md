# Scenario Format Specification

Scenarios define sequences of TC (Traffic Control) configurations applied over time. They enable automated network condition testing by progressively changing network parameters like latency, packet loss, and bandwidth.

## File Format

Scenarios use JSON5 format, which extends JSON with:
- Comments (`//` and `/* */`)
- Trailing commas
- Unquoted keys
- Human-readable syntax

## File Locations

Scenarios are loaded from the following directories (in priority order, later overrides earlier):

1. **System**: `/usr/share/tcgui/scenarios` - Package-installed scenarios
2. **User**: `~/.config/tcgui/scenarios` - User-defined scenarios
3. **Local**: `./scenarios` - Project-local scenarios

Files must have the `.json5` extension.

## Schema

```json5
{
    // Required: Unique identifier (used for references)
    id: "scenario-id",
    
    // Required: Human-readable name
    name: "Scenario Display Name",
    
    // Optional: Detailed description
    description: "What this scenario simulates",
    
    // Optional: Loop the scenario continuously (default: false)
    loop_scenario: false,
    
    // Optional: Restore original TC config on failure/abort (default: true)
    cleanup_on_failure: true,
    
    // Optional: Metadata for organization and display
    metadata: {
        tags: ["tag1", "tag2"],      // For filtering/categorization
        author: "Author Name",        // Creator attribution
        version: "1.0",               // Scenario version
    },
    
    // Required: Array of steps to execute in order
    steps: [
        {
            // Required: How long this step lasts
            duration: "30s",
            
            // Required: Description of this step
            description: "What happens during this step",
            
            // Required: TC configuration for this step
            tc_config: {
                // All fields are optional - presence enables the feature
                loss: { ... },
                delay: { ... },
                duplicate: { ... },
                reorder: { ... },
                corrupt: { ... },
                rate_limit: { ... },
            },
        },
        // ... more steps
    ],
}
```

## Duration Format

Durations support human-readable strings:

| Format | Example | Milliseconds |
|--------|---------|--------------|
| Milliseconds | `"500ms"` | 500 |
| Seconds | `"30s"` | 30,000 |
| Minutes | `"5m"` | 300,000 |
| Hours | `"1h"` | 3,600,000 |
| Compound | `"1m30s"` | 90,000 |
| Compound | `"1h30m"` | 5,400,000 |

## TC Configuration Fields

Each TC feature is optional. **Presence automatically enables the feature** - no need for `enabled: true`.

### Packet Loss

```json5
loss: {
    percentage: 5.0,      // 0.0-100.0: Packet loss percentage
    correlation: 25.0,    // 0.0-100.0: Correlation with previous packet (default: 0)
}
```

### Network Delay

```json5
delay: {
    base_ms: 100.0,       // 0.0-5000.0: Base delay in milliseconds
    jitter_ms: 20.0,      // 0.0-1000.0: Delay variation (default: 0)
    correlation: 25.0,    // 0.0-100.0: Jitter correlation (default: 0)
}
```

### Packet Duplication

```json5
duplicate: {
    percentage: 1.0,      // 0.0-100.0: Duplication percentage
    correlation: 0.0,     // 0.0-100.0: Correlation (default: 0)
}
```

### Packet Reordering

```json5
reorder: {
    percentage: 5.0,      // 0.0-100.0: Reorder percentage
    correlation: 0.0,     // 0.0-100.0: Correlation (default: 0)
    gap: 5,               // 1-10: Gap parameter (default: 5)
}
```

Note: Reordering requires some delay to function. If no delay is specified, a minimal 1ms delay is automatically added.

### Packet Corruption

```json5
corrupt: {
    percentage: 0.1,      // 0.0-100.0: Corruption percentage
    correlation: 0.0,     // 0.0-100.0: Correlation (default: 0)
}
```

### Rate Limiting

```json5
rate_limit: {
    rate_kbps: 1000,      // 1-1000000: Rate limit in kbps (default: 1000)
}
```

## Examples

### Basic Latency Test

```json5
{
    id: "latency-test",
    name: "High Latency Test",
    description: "Test application behavior under high latency conditions",
    
    steps: [
        {
            duration: "30s",
            description: "Normal latency",
            tc_config: {
                delay: { base_ms: 10 },
            },
        },
        {
            duration: "1m",
            description: "High latency",
            tc_config: {
                delay: { base_ms: 200, jitter_ms: 50 },
            },
        },
        {
            duration: "30s",
            description: "Cleanup - no TC config",
            tc_config: {},
        },
    ],
}
```

### Progressive Degradation

```json5
{
    id: "progressive-degradation",
    name: "Network Degradation Simulation",
    description: "Simulate progressively worsening network conditions",
    
    metadata: {
        tags: ["degradation", "testing"],
        author: "Test Team",
        version: "1.0",
    },
    
    steps: [
        {
            duration: "30s",
            description: "Good connection",
            tc_config: {
                delay: { base_ms: 5 },
            },
        },
        {
            duration: "30s",
            description: "Degraded connection",
            tc_config: {
                loss: { percentage: 2 },
                delay: { base_ms: 50, jitter_ms: 20 },
            },
        },
        {
            duration: "30s",
            description: "Poor connection",
            tc_config: {
                loss: { percentage: 10, correlation: 25 },
                delay: { base_ms: 150, jitter_ms: 75 },
                duplicate: { percentage: 1 },
            },
        },
    ],
}
```

### Looping Scenario

```json5
{
    id: "connection-flapping",
    name: "Connection Flapping",
    description: "Continuously alternate between good and bad connection",
    loop_scenario: true,  // Will repeat indefinitely
    
    steps: [
        {
            duration: "10s",
            description: "Good connection",
            tc_config: {},
        },
        {
            duration: "5s",
            description: "Connection issues",
            tc_config: {
                loss: { percentage: 30 },
                delay: { base_ms: 500 },
            },
        },
    ],
}
```

### Cleanup Step

An empty `tc_config: {}` removes all TC rules, restoring normal network behavior:

```json5
{
    duration: "30s",
    description: "Recovery - clear all TC rules",
    tc_config: {},
}
```

## Execution Behavior

### Step Transitions
- Steps execute sequentially
- Each step's TC configuration replaces the previous one
- The `duration` specifies how long the configuration is active before moving to the next step

### Loop Mode
- When `loop_scenario: true`, the scenario restarts after the last step
- Useful for continuous testing or stress testing
- Can be stopped manually via the UI

### Pause/Resume
- Scenarios can be paused during execution
- When paused, the current TC configuration remains active
- Resuming continues from where it left off

### Cleanup on Failure
- When `cleanup_on_failure: true` (default), the original TC state is restored if:
  - The scenario is manually stopped
  - An error occurs during execution
  - The backend disconnects
- Set to `false` to keep the last applied configuration on failure

### Multiple Interfaces
- A single scenario can be executed on multiple interfaces simultaneously
- Each interface maintains independent execution state
- The same scenario can run on different namespaces

## Validation

Scenarios are validated when loaded:
- `id` and `name` must be non-empty
- At least one step is required
- Duration strings must be valid
- TC parameter values must be within valid ranges

Invalid scenarios are skipped during loading with a warning logged.

## Built-in Scenarios

TC GUI includes several built-in scenarios in the `scenarios/` directory:

| Scenario | Description |
|----------|-------------|
| `mobile-degradation` | Mobile device moving away from base station |
| `network-congestion` | Daily network usage patterns |
| `intermittent-connectivity` | Connection drops and recovery |
| `quality-degradation` | Progressive quality degradation |
| `load-testing` | Network load testing patterns |
| `fast-degradation` | Quick degradation for testing |

These can be used as-is or as templates for custom scenarios.
