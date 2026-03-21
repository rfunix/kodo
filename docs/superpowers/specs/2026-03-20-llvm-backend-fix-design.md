# LLVM Backend Fix — Design Spec

**Date**: 2026-03-20
**Status**: Approved
**Scope**: Fix 4 bugs preventing valid LLVM IR generation

## Bugs

### Bug 1: SSA Register Sequencing (CRITICAL)
**Files**: `value.rs:346-402`, `terminator.rs:108-114`

`fresh_reg()` allocates `result_reg` before `cmp_reg` in comparison operations, producing out-of-order SSA numbering. LLVM requires sequential numbering.

**Bad**:
```llvm
%4 = icmp eq i64 %1, %2    ; %4 defined
%3 = zext i1 %4 to i64     ; %3 defined AFTER %4 — invalid!
```

**Fix**: Allocate `cmp_reg` before `result_reg` in `emit_binop()` for comparison ops. Same fix in `emit_terminator()` for branch conditions.

### Bug 2: Type Mismatch on Option/Result Parameters (CRITICAL)
**Files**: `function.rs`, `types.rs`

Functions like `Option_is_some()` receive `i64` parameter but body does `extractvalue { i64, [8 x i8] } %0` — type mismatch.

**Bad**:
```llvm
define i64 @Option_is_some(i64 %0) {
  %1 = extractvalue { i64, [8 x i8] } %0, 0  ; %0 is i64, not struct!
```

**Fix**: When parameter type is an unresolved enum (Option, Result), use the struct type `{ i64, [N x i8] }` consistently in both the function signature AND the body.

### Bug 3: Undeclared List Iterator Functions (CRITICAL)
**File**: `instruction.rs:174-319` (`resolve_runtime_name()`)

Missing mappings: `list_iter` → `kodo_list_iter`, `list_iterator_advance` → `kodo_list_iterator_advance`, etc.

**Fix**: Add all missing iterator function name mappings.

### Bug 4: Unreachable Basic Blocks (LOW)
Generated Option/Result functions have dead `bb4` blocks.

**Fix**: Skip empty/unreachable blocks during emission.

## Test Matrix

Every fix must be validated against these examples:

| Example | Features tested |
|---------|----------------|
| `hello.ko` | Basic fn, print, string literals |
| `fibonacci.ko` | Recursion, if/else, arithmetic |
| `while_loop.ko` | While loops, mutation |
| `enums.ko` | Enum variants, match, destructuring |
| `result_demo.ko` | Result<T,E>, Ok/Err, match |
| `option_demo.ko` | Option<T>, Some/None, match |
| `closures.ko` | Closures, higher-order functions |
| `testing.ko` | Test blocks, assertions |
| `green_threads.ko` | Spawn, green threads |

**For each example, run**:
```bash
PATH="/opt/homebrew/opt/llvm/bin:$PATH" kodoc build example.ko --backend=llvm
./example  # verify output matches Cranelift version
```

## Success Criteria

All 9 examples compile with `--backend=llvm` AND produce the same output as the Cranelift backend. No `llc` errors.
