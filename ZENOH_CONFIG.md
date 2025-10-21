# Zenoh Configuration

Both the tcgui-frontend and tcgui-backend now support configurable zenoh sessions through command line arguments. This allows you to deploy the applications in different network topologies and protocols.

## Command Line Arguments

### Frontend

```bash
tcgui-frontend --zenoh-mode <MODE> --zenoh-connect <ENDPOINTS> --zenoh-listen <ENDPOINTS>
```

### Backend

```bash
tcgui-backend --zenoh-mode <MODE> --zenoh-connect <ENDPOINTS> --zenoh-listen <ENDPOINTS>
```

### Arguments

- `--zenoh-mode <MODE>`: Session mode, either `peer` (default) or `client`
- `--zenoh-connect <ENDPOINTS>`: Comma-separated list of endpoints to connect to
- `--zenoh-listen <ENDPOINTS>`: Comma-separated list of endpoints to listen on (peer mode only)

## Zenoh Modes

### Peer Mode (Default)
Peer nodes can both connect to other nodes and accept incoming connections. They participate fully in the zenoh mesh network.

### Client Mode
Client nodes only connect to other nodes (typically routers or peers) and cannot accept incoming connections. They are leaf nodes in the zenoh topology.

## Endpoint Formats

Zenoh supports various transport protocols:

- `tcp/IP:PORT` - TCP transport (e.g., `tcp/192.168.1.1:7447`)
- `udp/IP:PORT` - UDP transport (e.g., `udp/192.168.1.1:7447`)
- `tcp/HOSTNAME:PORT` - TCP with hostname resolution
- `tls/IP:PORT` - TCP with TLS encryption
- `quic/IP:PORT` - QUIC transport

## Common Deployment Scenarios

### 1. Local Development (Default)
Both frontend and backend discover each other automatically:

```bash
# Terminal 1 - Backend
cargo run --bin tcgui-backend

# Terminal 2 - Frontend  
cargo run --bin tcgui-frontend
```

### 2. Peer-to-Peer over TCP
Direct TCP connection between frontend and backend:

```bash
# Terminal 1 - Backend (listening on port 7447)
cargo run --bin tcgui-backend -- --zenoh-listen tcp/0.0.0.0:7447

# Terminal 2 - Frontend (connecting to backend)
cargo run --bin tcgui-frontend -- --zenoh-connect tcp/192.168.1.100:7447
```

### 3. Client-Server Architecture
Backend as server, frontend as client:

```bash
# Terminal 1 - Backend (peer mode, listening)
cargo run --bin tcgui-backend -- --zenoh-mode peer --zenoh-listen tcp/0.0.0.0:7447

# Terminal 2 - Frontend (client mode, connecting)
cargo run --bin tcgui-frontend -- --zenoh-mode client --zenoh-connect tcp/192.168.1.100:7447
```

### 4. Multi-Backend with Zenoh Router
Multiple backends connecting through a zenoh router:

```bash
# Terminal 1 - Start zenoh router
zenohd --listen tcp/0.0.0.0:7447

# Terminal 2 - Backend 1
cargo run --bin tcgui-backend -- --name backend1 --zenoh-mode client --zenoh-connect tcp/localhost:7447

# Terminal 3 - Backend 2  
cargo run --bin tcgui-backend -- --name backend2 --zenoh-mode client --zenoh-connect tcp/localhost:7447

# Terminal 4 - Frontend
cargo run --bin tcgui-frontend -- --zenoh-mode client --zenoh-connect tcp/localhost:7447
```

### 5. Remote Deployment over Internet
Backend and frontend on different networks:

```bash
# On server (backend)
cargo run --bin tcgui-backend -- --zenoh-listen tcp/0.0.0.0:7447

# On client machine (frontend)
cargo run --bin tcgui-frontend -- --zenoh-connect tcp/your-server.com:7447
```

### 6. Multiple Endpoints
Connect to multiple zenoh nodes for redundancy:

```bash
cargo run --bin tcgui-frontend -- --zenoh-connect tcp/192.168.1.100:7447,tcp/192.168.1.101:7447
```

## Security Considerations

1. **TLS Transport**: Use `tls/` endpoints for encrypted communication over untrusted networks
2. **Firewall**: Ensure the specified ports are open in your firewall
3. **Network Access**: Bind to specific interfaces instead of `0.0.0.0` when possible
4. **Authentication**: Consider using zenoh's authentication features for production deployments

## Troubleshooting

### Connection Issues
1. Check if the specified ports are available: `netstat -ln | grep 7447`
2. Verify firewall rules allow the traffic
3. Test connectivity: `telnet <hostname> <port>`
4. Check zenoh logs for detailed error messages

### Discovery Issues
1. Ensure both nodes use compatible zenoh versions
2. Verify network connectivity between nodes
3. Check if multicast is working for automatic discovery
4. Use explicit connect/listen endpoints instead of relying on discovery

### Performance Issues
1. Use TCP for reliable, high-throughput scenarios
2. Use UDP for low-latency, multicast scenarios
3. Consider QUIC for scenarios requiring both reliability and low latency
4. Monitor network bandwidth usage

## Examples

### Basic TCP Setup
```bash
# Backend
sudo cargo run --bin tcgui-backend -- --zenoh-listen tcp/0.0.0.0:7447

# Frontend
cargo run --bin tcgui-frontend -- --zenoh-connect tcp/localhost:7447
```

### Secure TLS Setup
```bash
# Backend (requires TLS certificates)
sudo cargo run --bin tcgui-backend -- --zenoh-listen tls/0.0.0.0:7447

# Frontend  
cargo run --bin tcgui-frontend -- --zenoh-connect tls/your-server.com:7447
```

For more advanced zenoh configuration options, refer to the [Zenoh documentation](https://zenoh.io/docs/).