# Example Scenarios

Annotated examples demonstrating various scenario patterns and use cases.

## Example 1: Mobile Network Simulation

Simulates a mobile device moving away from a base station.

```json5
{
    // Unique identifier - used internally and for deduplication
    id: "mobile-degradation",
    
    // Display name shown in the UI
    name: "Mobile Device Distance Simulation",
    
    // Detailed description helps users understand the scenario
    description: "Simulate mobile device moving away from base station with progressive signal degradation",

    metadata: {
        // Tags for searching and filtering
        tags: ["mobile", "wireless", "degradation"],
        author: "TC GUI Built-in Templates",
        version: "1.0",
    },

    steps: [
        {
            // Step 1: Close to base station
            duration: "30s",
            description: "Close to base station - excellent signal quality",
            tc_config: {
                // Only delay - minimal impairment
                // Note: Just including 'delay' automatically enables it
                delay: { base_ms: 5 },
            },
        },
        {
            // Step 2: Starting to move away
            duration: "30s",
            description: "Moving away - signal degradation begins",
            tc_config: {
                // Slight packet loss appears
                loss: { percentage: 1 },
                // Latency increases
                delay: { base_ms: 20 },
            },
        },
        {
            // Step 3: Far from base station
            duration: "30s",
            description: "Far from base station - poor signal with high latency",
            tc_config: {
                // Significant loss with correlation (bursty)
                loss: { percentage: 15, correlation: 25 },
                // High latency with jitter
                delay: { base_ms: 100, jitter_ms: 50 },
            },
        },
        {
            // Step 4: Edge of coverage
            duration: "30s",
            description: "Edge of coverage - very poor connection",
            tc_config: {
                // Severe loss
                loss: { percentage: 25, correlation: 30 },
                // Very high latency and jitter
                delay: { base_ms: 200, jitter_ms: 100 },
                // Some packet duplication (common in poor wireless)
                duplicate: { percentage: 1 },
            },
        },
    ],
}
```

**Key Points**:
- Progressive degradation simulates real-world movement
- Correlation makes loss "bursty" like real wireless
- Jitter increases with distance (signal instability)
- Duplication can occur in poor wireless conditions

---

## Example 2: Network Congestion Pattern

Simulates typical daily network usage patterns.

```json5
{
    id: "network-congestion",
    name: "Network Congestion Simulation",
    description: "Simulate daily network usage patterns with varying congestion levels",

    metadata: {
        tags: ["congestion", "bandwidth", "daily-pattern"],
        author: "TC GUI Built-in Templates",
        version: "1.0",
    },

    steps: [
        {
            // Off-peak: Minimal users, network is fast
            duration: "1m",
            description: "Off-peak hours - minimal network congestion",
            tc_config: {
                loss: { percentage: 0.1 },
                delay: { base_ms: 10 },
            },
        },
        {
            // Morning rush: Users coming online
            duration: "1m30s",  // Compound duration format
            description: "Morning peak - increased network congestion",
            tc_config: {
                loss: { percentage: 2 },
                delay: { base_ms: 50, jitter_ms: 20 },
                duplicate: { percentage: 0.5 },
            },
        },
        {
            // Peak hours: Maximum load
            duration: "1m",
            description: "Peak congestion - maximum network load",
            tc_config: {
                loss: { percentage: 5, correlation: 15 },
                delay: { base_ms: 150, jitter_ms: 75 },
                duplicate: { percentage: 1.5 },
                // Bandwidth throttling during peak
                rate_limit: { rate_kbps: 1000 },
            },
        },
        {
            // Recovery: Congestion easing
            duration: "30s",
            description: "Recovery period - congestion decreasing",
            tc_config: {
                loss: { percentage: 1 },
                delay: { base_ms: 30, jitter_ms: 10 },
                // No rate limit - bandwidth restored
            },
        },
    ],
}
```

**Key Points**:
- Models real-world usage patterns
- Combines multiple impairments (loss, delay, duplication)
- Rate limiting simulates bandwidth contention
- Recovery phase shows return to normal

---

## Example 3: Intermittent Connectivity

Simulates unreliable network with periodic dropouts.

```json5
{
    id: "intermittent-connectivity",
    name: "Intermittent Connection Simulation",
    description: "Simulate unreliable network with periodic connectivity issues",
    
    // Loop this scenario indefinitely
    loop_scenario: true,

    metadata: {
        tags: ["unreliable", "dropout", "testing"],
        version: "1.0",
    },

    steps: [
        {
            // Most of the time: Connection is stable
            duration: "45s",
            description: "Stable connection period",
            tc_config: {
                // Clean network - empty config disables all impairments
            },
        },
        {
            // Brief dropout
            duration: "5s",
            description: "Connection dropout",
            tc_config: {
                // Severe loss simulates near-disconnect
                loss: { percentage: 80, correlation: 50 },
            },
        },
        {
            // Recovery with some residual issues
            duration: "10s",
            description: "Connection recovering",
            tc_config: {
                loss: { percentage: 10, correlation: 25 },
                delay: { base_ms: 50, jitter_ms: 30 },
            },
        },
    ],
}
```

**Key Points**:
- `loop_scenario: true` makes it repeat forever
- Empty `tc_config: {}` clears all impairments
- Short dropout (5s) followed by recovery
- Useful for testing reconnection logic

---

## Example 4: VoIP Quality Test

Tests voice call quality under stress.

```json5
{
    id: "voip-quality",
    name: "VoIP Quality Degradation Test",
    description: "Test voice quality with increasing network impairments",

    metadata: {
        tags: ["voip", "voice", "latency", "jitter"],
        author: "QA Team",
        version: "1.0",
    },
    
    // Restore network if test fails
    cleanup_on_failure: true,

    steps: [
        {
            // Baseline: Excellent call quality expected
            duration: "1m",
            description: "Baseline - excellent quality (MOS ~4.4)",
            tc_config: {
                delay: { base_ms: 20 },
                loss: { percentage: 0 },
            },
        },
        {
            // Good quality threshold
            duration: "1m",
            description: "Good quality threshold (MOS ~4.0)",
            tc_config: {
                delay: { base_ms: 50, jitter_ms: 10 },
                loss: { percentage: 0.5 },
            },
        },
        {
            // Acceptable quality threshold  
            duration: "1m",
            description: "Acceptable quality (MOS ~3.5)",
            tc_config: {
                delay: { base_ms: 100, jitter_ms: 30 },
                loss: { percentage: 1 },
            },
        },
        {
            // Poor quality - issues noticeable
            duration: "1m",
            description: "Poor quality (MOS ~3.0)",
            tc_config: {
                delay: { base_ms: 150, jitter_ms: 50 },
                loss: { percentage: 3 },
            },
        },
        {
            // Unacceptable - call would likely fail
            duration: "30s",
            description: "Unacceptable quality (MOS ~2.5)",
            tc_config: {
                delay: { base_ms: 200, jitter_ms: 80 },
                loss: { percentage: 5 },
            },
        },
    ],
}
```

**Key Points**:
- Each step maps to a VoIP quality level (MOS score)
- Jitter is critical for VoIP - increases with each step
- Gradual degradation helps identify quality thresholds
- Comments reference expected MOS scores

---

## Example 5: Rate Limiting Test

Tests application behavior under bandwidth constraints.

```json5
{
    id: "bandwidth-throttle",
    name: "Bandwidth Throttling Test",
    description: "Test application behavior under various bandwidth constraints",

    metadata: {
        tags: ["bandwidth", "throttle", "performance"],
        version: "1.0",
    },

    steps: [
        {
            duration: "30s",
            description: "Baseline - unlimited bandwidth",
            tc_config: {
                // No impairments
            },
        },
        {
            duration: "1m",
            description: "Broadband - 10 Mbps",
            tc_config: {
                rate_limit: { rate_kbps: 10000 },
            },
        },
        {
            duration: "1m",
            description: "DSL - 2 Mbps",
            tc_config: {
                rate_limit: { rate_kbps: 2000 },
                delay: { base_ms: 30 },  // DSL has some latency
            },
        },
        {
            duration: "1m",
            description: "Slow connection - 512 Kbps",
            tc_config: {
                rate_limit: { rate_kbps: 512 },
                delay: { base_ms: 50 },
            },
        },
        {
            duration: "1m",
            description: "Very slow - 128 Kbps (2G-like)",
            tc_config: {
                rate_limit: { rate_kbps: 128 },
                delay: { base_ms: 200 },
                loss: { percentage: 1 },
            },
        },
    ],
}
```

**Key Points**:
- Tests progressively slower connections
- Combines rate limiting with realistic latency
- Useful for testing loading states, timeouts
- Each step represents a common connection type

---

## Example 6: Minimal Scenario

Simplest possible scenario structure.

```json5
{
    id: "minimal",
    name: "Minimal Test",
    steps: [
        {
            duration: "10s",
            description: "Add 50ms delay",
            tc_config: {
                delay: { base_ms: 50 },
            },
        },
    ],
}
```

**Key Points**:
- Only required fields: `id`, `name`, `steps`
- Single step is valid
- Optional fields use defaults
- Good starting point for new scenarios

---

## Example 7: All Features Demonstration

Shows all available TC configuration options.

```json5
{
    id: "all-features",
    name: "All TC Features Demo",
    description: "Demonstrates all available traffic control features",
    
    loop_scenario: false,
    cleanup_on_failure: true,

    metadata: {
        tags: ["demo", "all-features", "reference"],
        author: "Documentation",
        version: "1.0",
    },

    steps: [
        {
            duration: "30s",
            description: "All features at moderate levels",
            tc_config: {
                // Packet loss with correlation
                loss: {
                    percentage: 2.0,     // 2% of packets dropped
                    correlation: 25.0,   // 25% correlated (bursty)
                },
                
                // Network delay with jitter
                delay: {
                    base_ms: 50.0,       // 50ms base delay
                    jitter_ms: 20.0,     // +/- 20ms variation
                    correlation: 25.0,   // 25% correlated
                },
                
                // Packet duplication
                duplicate: {
                    percentage: 0.5,     // 0.5% of packets duplicated
                    correlation: 0.0,    // Random duplication
                },
                
                // Packet reordering
                reorder: {
                    percentage: 1.0,     // 1% of packets reordered
                    correlation: 0.0,    // Random reordering
                    gap: 5,              // Reorder gap (packets)
                },
                
                // Packet corruption
                corrupt: {
                    percentage: 0.1,     // 0.1% of packets corrupted
                    correlation: 0.0,    // Random corruption
                },
                
                // Bandwidth rate limiting
                rate_limit: {
                    rate_kbps: 5000,     // 5 Mbps limit
                },
            },
        },
        {
            duration: "30s",
            description: "All features cleared",
            tc_config: {
                // Empty config disables everything
            },
        },
    ],
}
```

**Key Points**:
- Shows every available configuration option
- Comments explain each parameter
- Demonstrates clearing all settings with empty config
- Use as reference when building new scenarios

---

## Quick Reference

### Duration Formats
```
"500ms"   - 500 milliseconds
"30s"     - 30 seconds
"5m"      - 5 minutes
"1h"      - 1 hour
"1m30s"   - 1 minute 30 seconds
```

### Empty TC Config (Clear All)
```json5
tc_config: {}
```

### Enable Loop
```json5
loop_scenario: true
```

### Disable Cleanup on Failure
```json5
cleanup_on_failure: false
```

## See Also

- [Scenario Format](scenario-format.md) - Complete format specification
- [Best Practices](best-practices.md) - Guidelines for effective scenarios
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
