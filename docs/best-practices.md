# Scenario Best Practices

Guidelines for creating effective and realistic network scenarios.

## Scenario Design Principles

### 1. Start with Clear Objectives

Before creating a scenario, define:

- **What are you testing?** Application resilience, user experience, protocol behavior
- **What conditions are you simulating?** Mobile networks, congestion, satellite links
- **What's the expected outcome?** The behavior you want to observe or validate

### 2. Use Realistic Values

Research real-world network characteristics:

| Network Type | Typical Latency | Packet Loss | Bandwidth |
|--------------|-----------------|-------------|-----------|
| LAN | < 1ms | ~0% | 1 Gbps+ |
| WiFi (good) | 1-5ms | < 0.1% | 100+ Mbps |
| WiFi (poor) | 10-50ms | 1-5% | 10-50 Mbps |
| 4G LTE | 30-50ms | 0.1-1% | 10-50 Mbps |
| 3G | 100-500ms | 1-3% | 1-5 Mbps |
| Satellite | 500-700ms | 0.5-2% | 1-20 Mbps |
| Intercontinental | 100-300ms | ~0.1% | Varies |

### 3. Apply Gradual Changes

Avoid sudden, unrealistic jumps between conditions:

```json5
// Bad: Sudden jump from perfect to terrible
steps: [
    { duration: "30s", tc_config: { delay: { base_ms: 5 } } },
    { duration: "30s", tc_config: { delay: { base_ms: 500 } } },  // Too sudden!
]

// Good: Gradual degradation
steps: [
    { duration: "30s", tc_config: { delay: { base_ms: 5 } } },
    { duration: "30s", tc_config: { delay: { base_ms: 50 } } },
    { duration: "30s", tc_config: { delay: { base_ms: 150 } } },
    { duration: "30s", tc_config: { delay: { base_ms: 300 } } },
]
```

### 4. Use Correlation Appropriately

Correlation makes impairments more realistic by creating "bursty" behavior:

- **0% correlation**: Each packet is independently affected (random)
- **25-50% correlation**: Moderate bursts (typical for most scenarios)
- **75-100% correlation**: Strong bursts (severe network issues)

```json5
// Bursty packet loss (more realistic)
tc_config: {
    loss: { percentage: 5, correlation: 25 },
}

// Random packet loss (less realistic for most scenarios)
tc_config: {
    loss: { percentage: 5, correlation: 0 },
}
```

### 5. Combine Features Realistically

Network impairments often occur together:

```json5
// Realistic congested network
tc_config: {
    loss: { percentage: 2, correlation: 15 },
    delay: { base_ms: 100, jitter_ms: 30 },
    rate_limit: { rate_kbps: 5000 },
}

// Unrealistic: high loss without delay
tc_config: {
    loss: { percentage: 20 },  // Would have latency too
}
```

## Step Duration Guidelines

### Minimum Duration

- **Testing**: 10-30 seconds minimum per step
- **Demo/Presentation**: 30-60 seconds for visibility
- **Long-running tests**: 5+ minutes per step

### Step Count

- Keep scenarios manageable: 3-10 steps typically
- Each step should represent a distinct network state
- Avoid too many micro-steps (< 5 seconds)

### Total Duration

- Quick tests: 1-5 minutes
- Standard tests: 5-15 minutes  
- Extended tests: 15-60 minutes
- Maximum: 24 hours

## Common Patterns

### Progressive Degradation

Simulate conditions getting worse over time:

```json5
steps: [
    { duration: "1m", description: "Good", tc_config: { delay: { base_ms: 10 } } },
    { duration: "1m", description: "Degraded", tc_config: { delay: { base_ms: 50 } } },
    { duration: "1m", description: "Poor", tc_config: { delay: { base_ms: 150 } } },
    { duration: "1m", description: "Critical", tc_config: { delay: { base_ms: 300 } } },
]
```

### Recovery Pattern

Simulate degradation followed by recovery:

```json5
steps: [
    { duration: "30s", description: "Normal", tc_config: { delay: { base_ms: 10 } } },
    { duration: "1m", description: "Outage", tc_config: { loss: { percentage: 30 } } },
    { duration: "30s", description: "Recovering", tc_config: { loss: { percentage: 5 } } },
    { duration: "30s", description: "Restored", tc_config: { delay: { base_ms: 10 } } },
]
```

### Intermittent Issues

Simulate sporadic network problems:

```json5
loop_scenario: true,  // Repeat indefinitely
steps: [
    { duration: "45s", description: "Stable", tc_config: {} },
    { duration: "15s", description: "Hiccup", tc_config: { loss: { percentage: 10 } } },
]
```

### Bandwidth Throttling

Simulate limited bandwidth scenarios:

```json5
steps: [
    { duration: "1m", description: "Full speed", tc_config: {} },
    { duration: "2m", description: "Throttled", tc_config: { rate_limit: { rate_kbps: 1000 } } },
    { duration: "1m", description: "Restored", tc_config: {} },
]
```

## Testing Recommendations

### Test Your Scenarios

1. Run the scenario on a test interface first
2. Verify each step applies correctly (check with `tc qdisc show`)
3. Confirm timing is appropriate for your application
4. Test the `cleanup_on_failure` behavior

### Document Your Intent

Use descriptive names and descriptions:

```json5
// Good
{
    id: "voip-quality-test",
    name: "VoIP Call Quality Under Network Stress",
    description: "Tests voice quality degradation with increasing latency and jitter",
}

// Poor
{
    id: "test1",
    name: "Network Test",
    description: "",
}
```

### Use Meaningful Tags

Tags help organize and find scenarios:

```json5
metadata: {
    tags: [
        "voip",           // Application type
        "latency",        // Primary impairment
        "production",     // Environment
        "critical",       // Priority
    ],
}
```

## What to Avoid

### Unrealistic Extremes

```json5
// Avoid: 99% packet loss is essentially a disconnect
tc_config: { loss: { percentage: 99 } }

// Avoid: 5 second delay is unrealistic for most networks
tc_config: { delay: { base_ms: 5000 } }
```

### Conflicting Impairments

```json5
// Avoid: Very high loss with very high reorder
// (packets that are lost can't be reordered)
tc_config: {
    loss: { percentage: 50 },
    reorder: { percentage: 50 },
}
```

### Very Short Steps

```json5
// Avoid: Steps too short for meaningful testing
steps: [
    { duration: "1s", ... },
    { duration: "2s", ... },
]
```

### Overly Complex Scenarios

```json5
// Avoid: Too many steps make scenarios hard to understand
steps: [
    // 50 steps with subtle variations...
]

// Better: Group similar conditions, use fewer distinct steps
```

## Performance Considerations

### CPU Impact

Heavy impairments can increase CPU usage:

- Rate limiting: Moderate CPU impact
- High packet loss with correlation: Higher CPU impact
- Complex combinations: Cumulative impact

### Memory Usage

The scenario execution engine maintains:

- Pre-execution TC state (for rollback)
- Execution statistics
- Step timing information

This is minimal for most scenarios.

### Network Impact

Remember that impairments affect real traffic:

- Test on isolated interfaces when possible
- Use network namespaces for isolation
- Be careful with production interfaces

## See Also

- [Scenario Format](scenario-format.md) - Complete format specification
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [Examples](examples.md) - Annotated example scenarios
