# MCP Server

Kōdo ships with a built-in [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that exposes the compiler as a set of tools AI agents can invoke natively.

## Quick Start

```bash
# Build the MCP server
cargo build -p kodo_mcp

# The binary is `kodo-mcp`
./target/debug/kodo-mcp
```

The server reads **newline-delimited JSON-RPC 2.0** requests from stdin and writes responses to stdout, following the MCP stdio transport.

## Configuration

Add `kodo-mcp` to your AI agent's MCP configuration. For example, in Claude Desktop:

```json
{
  "mcpServers": {
    "kodo": {
      "command": "/path/to/kodo-mcp"
    }
  }
}
```

## Available Tools

The server exposes 6 tools:

### `kodo.check`

Type-check Kōdo source code and return structured errors with fix patches and repair plans.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `source` | string | yes | Kōdo source code to check |
| `filename` | string | no | Optional filename for error reporting |

**Response (success):**

```json
{
  "status": "ok",
  "module": "example",
  "errors": [],
  "warnings": []
}
```

**Response (errors):**

```json
{
  "status": "failed",
  "errors": [
    {
      "code": "E0200",
      "message": "type mismatch: expected `Int`, found `String`",
      "span": { "start": 42, "end": 48 },
      "suggestion": "change the value to match type `Int`",
      "fix_patch": {
        "description": "change type to `Int`",
        "start_offset": 42,
        "end_offset": 48,
        "replacement": "42"
      }
    }
  ]
}
```

### `kodo.build`

Compile source through the full pipeline: parse → type-check → contracts → desugar → MIR. Native codegen is not available in the MCP context (source-only), but MIR success proves the program is well-formed through all compiler phases.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `source` | string | yes | Kōdo source code to compile |

**Response (success):**

```json
{
  "status": "ok",
  "module": "example",
  "phase": "mir",
  "message": "compilation successful (through MIR)",
  "function_count": 3
}
```

### `kodo.fix`

Collect auto-fix patches and multi-step repair plans for all errors in the source. This is the core tool for the **error → fix → recompile** agent loop.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `source` | string | yes | Kōdo source code to fix |

**Response (errors found):**

```json
{
  "status": "errors_found",
  "error_count": 1,
  "patches": [
    {
      "error_code": "E0200",
      "description": "change type to `Int`",
      "start_offset": 74,
      "end_offset": 94,
      "replacement": "Int"
    }
  ],
  "repair_plans": [
    {
      "error_code": "E0200",
      "message": "type mismatch",
      "steps": [
        {
          "id": 0,
          "description": "wrap in Result::Ok",
          "patches": [{ "start_offset": 10, "end_offset": 15, "replacement": "Result::Ok(val)" }]
        }
      ]
    }
  ]
}
```

**Response (no errors):**

```json
{
  "status": "ok",
  "message": "no errors to fix",
  "patches": [],
  "repair_plans": []
}
```

### `kodo.describe`

Return module metadata: functions (with signatures, contracts, annotations), type declarations, and meta block.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `source` | string | yes | Kōdo source code to describe |

### `kodo.explain`

Explain a compiler error code with description, common causes, and fix suggestions.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `code` | string | yes | Error code (e.g. `E0200`) |

### `kodo.confidence_report`

Generate a confidence report for all functions in a module, based on `@confidence` and `@reviewed_by` annotations.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `source` | string | yes | Kōdo source code to analyze |

## Protocol Details

### Supported Methods

| Method | Description |
|--------|-------------|
| `initialize` | Handshake — returns server info and capabilities |
| `tools/list` | Returns all 6 tool definitions with JSON Schema |
| `tools/call` | Invokes a tool by name with arguments |

### Error Codes

Standard JSON-RPC 2.0 error codes:

| Code | Meaning |
|------|---------|
| `-32700` | Parse error (malformed JSON) |
| `-32601` | Method not found / unknown tool |
| `-32602` | Invalid params (missing required parameter) |

### Example Session

```bash
# Initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | kodo-mcp

# List tools
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | kodo-mcp

# Check source
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kodo.check","arguments":{"source":"module test {\n    meta { purpose: \"test\" }\n    fn main() -> Int {\n        return 42\n    }\n}\n"}}}' | kodo-mcp
```

## Agent Integration Pattern

The recommended agent workflow with Kōdo MCP:

1. **Write** — agent generates `.ko` source
2. **Check** — call `kodo.check` to validate
3. **Fix** — if errors, call `kodo.fix` to get patches
4. **Apply** — apply patches to source (byte offsets)
5. **Build** — call `kodo.build` to verify full pipeline
6. **Repeat** until `status: "ok"`

For complex errors, `repair_plans` provide multi-step sequences. Each step has an `id` and patches should be applied in order.
