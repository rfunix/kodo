# LLVM Backend — Design Spec

**Date**: 2026-03-20
**Status**: Approved
**Scope**: Milestone 7 — new crate `kodo_codegen_llvm` emitting LLVM IR textual

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| LLVM integration | Emit textual `.ll` files | Zero LLVM library dependency, debuggable output |
| Crate structure | Separate `kodo_codegen_llvm` | No risk to existing Cranelift backend |
| MIR coverage | Complete (100%) | Runtime is C ABI — all features are just `call` instructions |

## Architecture

```
MIR (kodo_mir)
    ├──→ kodo_codegen (Cranelift) → .o → link → binary  [default]
    └──→ kodo_codegen_llvm (new)  → .ll → llc → .o → link → binary  [--backend=llvm]
```

## New Crate: `kodo_codegen_llvm`

**Location**: `crates/kodo_codegen_llvm/`

**Dependencies**: `kodo_mir`, `kodo_types` (for Type enum). No LLVM library deps.

**Public API:**
```rust
pub fn compile_module_to_llvm_ir(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    vtable_defs: &HashMap<(String, String), Vec<String>>,
    options: &LLVMCodegenOptions,
) -> Result<String, CodegenError>
```

Returns a complete `.ll` file as a String.

**Options:**
```rust
pub struct LLVMCodegenOptions {
    pub target_triple: String,  // e.g. "x86_64-unknown-linux-gnu"
    pub opt_level: u8,          // 0-3
}
```

## Pipeline in kodoc

```bash
kodoc build app.ko --backend=llvm
```

1. Parse → Type check → MIR (same pipeline as Cranelift)
2. `kodo_codegen_llvm::compile_module_to_llvm_ir()` → `output.ll`
3. `llc -filetype=obj -O2 output.ll -o output.o` (requires `llc` in PATH)
4. `cc output.o -lkodo_runtime -o app` (same linker as Cranelift)

Error if `llc` not found: "LLVM backend requires `llc` in PATH. Install LLVM or use default Cranelift backend."

`--release` is alias for `--backend=llvm`.

## MIR → LLVM IR Mapping

### Types
```llvm
; Primitives
i64          ; Int, Bool (0/1), pointers
double       ; Float64
{ i64, i64 } ; String (ptr, len)

; Structs
%Point = type { i64, i64 }

; Enums: discriminant i64 + max payload size
; e.g. Option<Int>: { i64, i64 } = { disc, payload }
```

### Instructions
```llvm
; Assign constant
%0 = add i64 42, 0

; Assign from local
%1 = add i64 %0, 0

; BinOp
%2 = add i64 %0, %1
%3 = sub i64 %0, %1
%4 = mul i64 %0, %1
%5 = sdiv i64 %0, %1

; Call
%6 = call i64 @user_function(i64 %0, i64 %1)

; Void call (runtime builtin)
call void @kodo_green_maybe_yield()

; Yield (same as void call)
call void @kodo_green_maybe_yield()
```

### Control Flow
```llvm
; Branch
br i1 %cond, label %then, label %else

; Goto
br label %target

; Return
ret i64 %val
ret void
```

### Structs
```llvm
; Alloca
%point = alloca %Point
%ptr = getelementptr %Point, %Point* %point, i32 0, i32 0
store i64 %x, i64* %ptr
```

### Enums
```llvm
; Enum stored as { i64 discriminant, [payload bytes] }
; Stack-allocated via alloca
%opt = alloca { i64, i64 }
%disc_ptr = getelementptr { i64, i64 }, { i64, i64 }* %opt, i32 0, i32 0
store i64 0, i64* %disc_ptr  ; discriminant = 0 (Some)
```

### Runtime Declarations
```llvm
; All runtime functions declared as external
declare i64 @kodo_main()
declare void @kodo_green_init(i64)
declare void @kodo_green_spawn(i64)
declare void @kodo_green_maybe_yield()
declare i64 @kodo_future_new()
declare void @kodo_future_complete(i64, i64)
declare i64 @kodo_future_await(i64)
; ... all builtins
```

## Files

### New
- `crates/kodo_codegen_llvm/Cargo.toml`
- `crates/kodo_codegen_llvm/src/lib.rs` — public API + LLVMCodegenOptions
- `crates/kodo_codegen_llvm/src/emitter.rs` — LLVM IR string builder
- `crates/kodo_codegen_llvm/src/types.rs` — Type → LLVM type string mapping
- `crates/kodo_codegen_llvm/src/function.rs` — MirFunction → LLVM function
- `crates/kodo_codegen_llvm/src/instruction.rs` — Instruction → LLVM IR
- `crates/kodo_codegen_llvm/src/terminator.rs` — Terminator → LLVM IR
- `crates/kodo_codegen_llvm/src/builtins.rs` — runtime function declarations

### Modified
- `Cargo.toml` — add kodo_codegen_llvm to workspace members
- `crates/kodoc/Cargo.toml` — add kodo_codegen_llvm dependency
- `crates/kodoc/src/main.rs` — add `--backend` flag
- `crates/kodoc/src/commands/build.rs` — dispatch to LLVM backend when flagged

## Non-Goals (v1)
- LLVM library linking (inkwell/llvm-sys)
- Debug info (DWARF) generation
- LTO (Link-Time Optimization)
- Custom LLVM passes
- JIT compilation
