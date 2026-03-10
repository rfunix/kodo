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

| Intent | Description |
|--------|-------------|
| `console_app` | Console application with greeting message |
| `math_module` | Mathematical helper functions from declarations |

### Planned

| Intent | Description |
|--------|-------------|
| `serve_http` | HTTP server with routing |
| `database` | Database connection with migrations |
| `json_api` | JSON REST API endpoint |
| `file_io` | File system operations |
| `cache` | In-memory or external caching |
| `queue` | Message queue consumer/producer |
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
