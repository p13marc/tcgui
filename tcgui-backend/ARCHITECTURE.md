# TC GUI Backend Architecture

## Current Architecture (Before Refactoring)

```mermaid
graph TD
    A[main.rs - Monolithic Backend] --> B[TcBackend Struct]
    B --> C[Query Handlers Embedded]
    B --> D[TC Command Manager]
    B --> E[Network Manager]
    B --> F[Bandwidth Monitor]
    
    C --> G[Zenoh Session Management]
    C --> H[Message Processing]
    C --> I[Response Generation]
    
    D --> J[TC Commands Module]
    E --> K[Network Module]
    F --> L[Bandwidth Module]
    
    M[Configuration] --> A
    N[CLI Args] --> A
    O[Error Handling] --> A
    
    style A fill:#ffcccc
    style B fill:#ffcccc
    style C fill:#ffcccc
```

### Problems with Current Architecture

1. **Single Responsibility Violation**: `TcBackend` handles too many concerns
2. **Tight Coupling**: Direct dependencies between all components
3. **Limited Testability**: Hard to mock dependencies
4. **Configuration Scattered**: No centralized config management
5. **Error Handling Inconsistent**: Different error patterns throughout
6. **No Separation**: Query handling mixed with business logic

## Target Architecture (After Refactoring)

```mermaid
graph TD
    subgraph "Application Layer"
        A[main.rs - Application Entry Point]
        B[Service Container - DI]
        C[Configuration Manager]
    end
    
    subgraph "Handler Layer"
        D[TC Query Handler]
        E[Interface Query Handler]
        F[Handler Middleware]
    end
    
    subgraph "Service Layer"
        G[TC Service]
        H[Network Service]
        I[Bandwidth Service]
        J[Health Service]
    end
    
    subgraph "Repository Layer"
        K[Interface Repository]
        L[TC Configuration Repository]
        M[Bandwidth Statistics Repository]
    end
    
    subgraph "Infrastructure Layer"
        N[Command Execution]
        O[Zenoh Messaging]
        P[System Resources]
        Q[Event Bus]
    end
    
    subgraph "Cross-cutting Concerns"
        R[Error Handling]
        S[Logging & Metrics]
        T[Validation]
        U[Caching]
    end
    
    A --> B
    B --> C
    B --> D
    B --> E
    
    D --> F
    E --> F
    F --> G
    F --> H
    F --> I
    
    G --> K
    G --> L
    H --> K
    I --> M
    
    G --> N
    H --> N
    I --> N
    
    G --> Q
    H --> Q
    I --> Q
    
    K --> P
    L --> P
    M --> P
    
    G --> J
    H --> J
    I --> J
    
    R -.-> D
    R -.-> E
    R -.-> G
    R -.-> H
    R -.-> I
    
    S -.-> D
    S -.-> E
    S -.-> G
    S -.-> H
    S -.-> I
    
    T -.-> D
    T -.-> E
    T -.-> G
    T -.-> H
    T -.-> I
    
    U -.-> K
    U -.-> L
    U -.-> M
    
    style A fill:#ccffcc
    style B fill:#ccffcc
    style C fill:#ccffcc
    style D fill:#cceeff
    style E fill:#cceeff
    style F fill:#cceeff
    style G fill:#ffffcc
    style H fill:#ffffcc
    style I fill:#ffffcc
    style J fill:#ffffcc
```

## Layer Responsibilities

### Application Layer
- **main.rs**: Application bootstrap, dependency injection setup
- **Service Container**: Manages service lifecycles and dependencies
- **Configuration Manager**: Centralized configuration management

### Handler Layer
- **Query Handlers**: Process Zenoh queries and route to services
- **Middleware**: Cross-cutting concerns (logging, validation, error handling)

### Service Layer
- **TC Service**: Traffic control business logic
- **Network Service**: Network interface management
- **Bandwidth Service**: Monitoring and statistics
- **Health Service**: System health and diagnostics

### Repository Layer
- **Interface Repository**: Network interface state management
- **TC Repository**: Traffic control configuration persistence
- **Bandwidth Repository**: Statistics storage and retrieval

### Infrastructure Layer
- **Command Execution**: System command execution abstraction
- **Zenoh Messaging**: Message bus communication
- **System Resources**: OS-level resource access
- **Event Bus**: Internal event-driven communication

## Data Flow Architecture

```mermaid
sequenceDiagram
    participant Client as Frontend Client
    participant ZH as Zenoh Hub
    participant QH as Query Handler
    participant MW as Middleware
    participant SVC as Service Layer
    participant REPO as Repository Layer
    participant CMD as Command Layer
    participant SYS as System
    
    Client->>ZH: TC Apply Request
    ZH->>QH: Route Query
    QH->>MW: Process Request
    MW->>MW: Validate & Log
    MW->>SVC: Business Logic
    SVC->>REPO: Get Current State
    REPO-->>SVC: Current Config
    SVC->>CMD: Generate Command
    CMD->>SYS: Execute TC Command
    SYS-->>CMD: Command Result
    CMD-->>SVC: Result
    SVC->>REPO: Update State
    SVC-->>MW: Response
    MW-->>QH: Response
    QH-->>ZH: Query Response
    ZH-->>Client: Response
    
    Note over SVC: Event Bus Notification
    SVC->>SVC: Publish TC Event
    SVC->>SVC: Update Subscribers
```

## Service Dependencies

```mermaid
graph LR
    subgraph "Service Dependencies"
        A[TC Service] --> B[TC Repository]
        A --> C[Command Executor]
        A --> D[Event Bus]
        
        E[Network Service] --> F[Interface Repository]
        E --> C
        E --> D
        
        G[Bandwidth Service] --> H[Bandwidth Repository]
        G --> C
        G --> D
        
        I[Health Service] --> A
        I --> E
        I --> G
        
        J[Configuration Manager] --> K[Config Repository]
        
        L[Service Container] --> A
        L --> E
        L --> G
        L --> I
        L --> J
        
        M[Query Handlers] --> A
        M --> E
        M --> G
        
        N[Middleware] --> O[Validation Service]
        N --> P[Logging Service]
        N --> Q[Error Handler]
    end
    
    style A fill:#e1f5fe
    style E fill:#e1f5fe
    style G fill:#e1f5fe
    style I fill:#e1f5fe
```

## Event-Driven Communication

```mermaid
graph TD
    subgraph "Event Publishers"
        A[TC Service]
        B[Network Service]
        C[Bandwidth Service]
    end
    
    subgraph "Event Bus"
        D[Internal Event Bus]
    end
    
    subgraph "Event Subscribers"
        E[Health Service]
        F[Metrics Collector]
        G[Cache Manager]
        H[Zenoh Publisher]
    end
    
    A -->|TC Events| D
    B -->|Network Events| D
    C -->|Bandwidth Events| D
    
    D -->|Subscribe| E
    D -->|Subscribe| F
    D -->|Subscribe| G
    D -->|Subscribe| H
    
    subgraph "Event Types"
        I[TcConfigApplied]
        J[TcConfigRemoved]
        K[InterfaceDiscovered]
        L[InterfaceRemoved]
        M[BandwidthMeasured]
        N[ServiceHealthChanged]
    end
```

## Configuration Management

```mermaid
graph TD
    subgraph "Configuration Sources"
        A[CLI Arguments]
        B[Environment Variables]
        C[Config Files]
        D[Runtime Overrides]
    end
    
    subgraph "Configuration Manager"
        E[Config Builder]
        F[Config Validator]
        G[Config Watcher]
    end
    
    subgraph "Configuration Consumers"
        H[Service Container]
        I[Zenoh Session]
        J[Logger]
        K[Services]
    end
    
    A --> E
    B --> E
    C --> E
    D --> E
    
    E --> F
    F --> G
    
    G --> H
    G --> I
    G --> J
    G --> K
    
    G -.->|Hot Reload| G
```

## Benefits of New Architecture

### Separation of Concerns
- Each layer has a single, well-defined responsibility
- Business logic separated from infrastructure concerns
- Query handling separated from business logic

### Testability
- Dependency injection enables easy mocking
- Services can be tested in isolation
- Repository pattern allows database mocking

### Maintainability
- Clear module boundaries
- Consistent patterns across layers
- Easy to locate and modify functionality

### Scalability
- Services can be scaled independently
- Event-driven architecture supports loose coupling
- Caching can be added at repository layer

### Observability
- Health checks at every layer
- Comprehensive logging and metrics
- Distributed tracing support

## Implementation Notes

### Technology Stack
- **Rust**: Core language
- **Tokio**: Async runtime
- **Zenoh**: Distributed communication
- **Tracing**: Structured logging
- **Serde**: Serialization
- **Anyhow/ThisError**: Error handling

### Key Design Patterns
- **Dependency Injection**: Service container pattern
- **Repository Pattern**: Data access abstraction
- **Command Pattern**: Command execution abstraction
- **Observer Pattern**: Event-driven communication
- **Builder Pattern**: Configuration and command building

### Performance Considerations
- **Connection pooling** for system resources
- **Intelligent caching** at repository layer
- **Batch processing** for bulk operations
- **Async/await** throughout for non-blocking operations

This architecture provides a solid foundation for a maintainable, testable, and scalable TC GUI backend system.