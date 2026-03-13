# Kōdo Intent System

## Overview

The intent system is Kōdo's most distinctive feature. It bridges the gap between
AI agents' high-level reasoning and concrete executable code.

Agents declare **WHAT** should happen using `intent` blocks. The compiler's
resolver maps those declarations to concrete implementations, then verifies
the generated code satisfies all contracts.

## Why Intents?

Traditional programming requires agents to generate exact implementation details:
connection pools, error handling, protocol negotiation, etc. This creates surface
area for bugs and forces agents to reason about low-level concerns.

With intents, agents express goals. The compiler handles the rest.

```
// Instead of 50 lines of HTTP server setup...
intent serve_http {
    port: 8080
    routes: [
        GET "/health" => health_check
    ]
}
```

## How It Works

```
1. Agent writes `intent` block with configuration
        │
        ▼
2. Parser produces IntentDecl AST node
        │
        ▼
3. Resolver looks up matching ResolverStrategy
        │
        ▼
4. Strategy generates concrete Function AST nodes
        │
        ▼
5. Generated code is type-checked and contract-verified
        │
        ▼
6. If verification passes, code enters the compilation pipeline
```

## Built-in Resolvers

### Implemented

| Intent | Description | Config Keys |
|--------|-------------|-------------|
| `console_app` | Console application with greeting message | `greeting`, `entry_point` |
| `math_module` | Mathematical helper functions from declarations | `functions` |
| `serve_http` | HTTP server with routing (legacy) | `port`, `routes` |
| `database` | Database connection with table/query stubs | `driver`, `tables`, `queries` |
| `json_api` | JSON REST API (stubs or real HTTP server) | `routes`, `models`, `port`, `base_path`, `endpoints` |
| `cache` | In-memory caching with get/set/invalidate | `strategy`, `max_size` |
| `queue` | Message queue producer/consumer | `backend`, `topics` |
| `cli` | CLI tool with command dispatch | `name`, `version`, `commands` |
| `http_server` | Real HTTP server with route dispatch | `port`, `routes`, `not_found` |
| `file_processor` | File processing pipeline | `input`, `output`, `transform` |
| `worker` | Worker loop with error handling | `task`, `max_iterations`, `on_error` |

### Planned

| Intent | Description |
|--------|-------------|
| `scheduler` | Periodic task execution |

## Custom Resolvers

Developers can define custom resolver strategies:

```
resolver my_cache_resolver for intent cache {
    fn resolve(config: CacheConfig) -> impl CacheProvider {
        // Concrete implementation that satisfies CacheProvider trait
    }
}
```

## Verification

Every resolved implementation must satisfy:

1. The intent's declared contracts (if any)
2. Type safety of all generated code
3. No ownership violations in generated code
4. All called functions' preconditions

If any verification fails, the compiler emits an E0401 error pointing to the
intent block, with details about which contract was violated.

## Human Auditability

A key design goal: humans must be able to understand what intents resolve to.

- `kodoc intent-explain <file>` shows what each intent resolves to
- Generated code is included in documentation output
- Intent resolution is deterministic — same input always produces same output
- No "magic" — every resolver step is traceable
