# Scenario Troubleshooting Guide

Common issues and solutions when working with TC GUI scenarios.

## Scenario Loading Issues

### Scenario Not Appearing in List

**Symptoms**: Scenario file exists but doesn't show in the UI.

**Possible Causes**:

1. **Invalid JSON5 syntax**
   ```bash
   # Validate your JSON5 file
   cat your-scenario.json5 | json5  # Requires json5 CLI tool
   ```
   
   Common syntax errors:
   - Missing commas between fields
   - Mismatched braces or brackets
   - Invalid escape sequences in strings

2. **Wrong file extension**
   - Files must end in `.json5`
   - Case sensitive on Linux: `.JSON5` won't work

3. **Wrong directory**
   - Check scenario is in one of: `./scenarios`, `~/.config/tcgui/scenarios`, `/usr/share/tcgui/scenarios`
   - Use `--no-default-scenarios` to verify which directories are being loaded

4. **Validation failure**
   - Check backend logs for validation errors:
     ```bash
     journalctl -u tcgui-backend -f  # If running as service
     # Or check terminal output if running manually
     ```

### Parse Error Messages

**"Invalid duration 'X'"**
```json5
// Wrong
{ duration: "30", ... }        // Missing unit
{ duration: "1.5s", ... }      // Decimals not supported
{ duration: "30 seconds", ... } // Full words not supported

// Correct
{ duration: "30s", ... }
{ duration: "1500ms", ... }
{ duration: "1m30s", ... }
```

**"Scenario field 'X' cannot be empty"**
```json5
// Wrong
{ id: "", name: "Test", ... }

// Correct
{ id: "my-scenario", name: "Test", ... }
```

**"Validation error in step N"**
```json5
// Check step N (0-indexed) for:
// - Empty description
// - Zero duration
// - Invalid TC parameter values
```

## Execution Issues

### Scenario Won't Start

**"Scenario already running on interface X"**

Only one scenario can run per interface at a time.

Solution:
- Stop the existing scenario first
- Or select a different interface

**"Interface not found"**

The target interface doesn't exist or isn't accessible.

Solution:
- Verify interface exists: `ip link show`
- Check network namespace: `ip netns exec <ns> ip link show`
- Ensure backend has CAP_NET_ADMIN capability

**"Execution query channel not available"**

Backend communication issue.

Solution:
- Verify backend is running and connected
- Check Zenoh connectivity
- Restart frontend and/or backend

### Scenario Stops Unexpectedly

**Check execution state in UI**:
- **Completed**: Scenario finished normally
- **Stopped**: User stopped the scenario
- **Failed**: Error occurred during execution

**For failures, check**:
1. Backend logs for TC command errors
2. Interface still exists and is up
3. Sufficient permissions (CAP_NET_ADMIN)

### TC Commands Not Applied

**Symptoms**: Scenario runs but network isn't affected.

**Verify TC is being applied**:
```bash
# Check current qdisc on interface
tc qdisc show dev eth0

# Should show netem qdisc with your parameters
# e.g., "qdisc netem 8001: root limit 1000 delay 100ms"
```

**Common causes**:

1. **Wrong interface selected**
   - Verify you selected the correct interface in the UI

2. **Interface in wrong namespace**
   - Ensure namespace selection matches

3. **TC parameters too small to notice**
   - 1ms delay might not be perceptible
   - 0.1% loss might need many packets to observe

4. **Traffic not going through interface**
   - Verify routing: `ip route`
   - Check traffic is actually using the interface

### Cleanup Not Working

**Symptoms**: Network impairments persist after scenario stops.

**Manual cleanup**:
```bash
# Remove netem qdisc from interface
sudo tc qdisc del dev eth0 root

# Or for namespaced interface
sudo ip netns exec myns tc qdisc del dev veth0 root
```

**Check `cleanup_on_failure` setting**:
```json5
{
    cleanup_on_failure: true,  // Should restore on failure
    // ...
}
```

Note: Cleanup always runs when user clicks Stop, regardless of this setting.

## Performance Issues

### High CPU Usage During Execution

**Causes**:
- Rate limiting with high throughput
- Many active scenarios simultaneously
- Very short step durations (< 1 second)

**Solutions**:
- Reduce number of concurrent scenarios
- Increase step durations
- Consider if rate limiting is necessary

### UI Becomes Unresponsive

**Causes**:
- Too many active executions
- Backend publishing updates too frequently
- Network connectivity issues to backend

**Solutions**:
- Reduce active execution count
- Check system resources
- Restart frontend

### Scenario Timing Inaccurate

**Symptoms**: Steps don't last as long as specified.

**Causes**:
- System under heavy load
- Clock synchronization issues
- Pause/resume affecting timing

**Note**: Pause duration is tracked and subtracted from step timing to maintain accuracy.

## Backend Connection Issues

### "No Backends Connected"

**Check backend is running**:
```bash
# Check process
pgrep -f tcgui-backend

# Check it has capabilities
getcap $(which tcgui-backend)
# Should show: cap_net_admin=ep
```

**Check Zenoh connectivity**:
- Both frontend and backend must reach Zenoh router
- Default: connects to local Zenoh or uses peer-to-peer
- Check firewall rules for Zenoh ports (7447, 8000)

### Backend Disconnects Frequently

**Check**:
- Network stability between frontend and backend
- Backend logs for errors
- System resources (memory, CPU)

## Scenario File Issues

### Duplicate Scenario IDs

**Symptoms**: Only one scenario appears, or wrong scenario loads.

**Cause**: Multiple files with same `id` field.

**Solution**: Ensure unique IDs across all scenario directories.

```bash
# Find duplicate IDs
grep -r '"id":' ~/.config/tcgui/scenarios/ ./scenarios/
```

### Scenario Not Updating After Edit

**Cause**: Scenarios are loaded at backend startup.

**Solution**: 
- Click "Refresh" button in UI
- Or restart backend to reload all scenarios

### Permission Denied on Scenario Directory

```bash
# Check permissions
ls -la ~/.config/tcgui/scenarios/

# Fix if needed
chmod 755 ~/.config/tcgui/scenarios/
chmod 644 ~/.config/tcgui/scenarios/*.json5
```

## Debugging Tips

### Enable Debug Logging

**Backend**:
```bash
RUST_LOG=debug tcgui-backend
```

**Frontend**:
```bash
RUST_LOG=debug tcgui-frontend
```

### Verify TC Commands

Watch TC commands being executed:
```bash
# In another terminal, monitor TC changes
watch -n 0.5 'tc qdisc show dev eth0'
```

### Test Scenario Manually

Apply TC settings manually to verify they work:
```bash
# Add netem qdisc
sudo tc qdisc add dev eth0 root netem delay 100ms loss 5%

# Verify
tc qdisc show dev eth0

# Remove
sudo tc qdisc del dev eth0 root
```

### Check Network Namespace

```bash
# List namespaces
ip netns list

# Execute command in namespace
sudo ip netns exec myns tc qdisc show
```

## Getting Help

If issues persist:

1. Check backend and frontend logs with debug logging enabled
2. Verify TC commands work manually
3. Test with a minimal scenario (single step, single feature)
4. Report issues at: https://github.com/anthropics/claude-code/issues

## See Also

- [Scenario Format](scenario-format.md) - Complete format specification
- [Best Practices](best-practices.md) - Guidelines for effective scenarios
- [Examples](examples.md) - Working example scenarios
