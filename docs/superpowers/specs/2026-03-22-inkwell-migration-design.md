# Design Spec: Migrate LLVM Backend from Textual IR to Inkwell API

## Context

The Kōdo LLVM backend (`kodo_codegen_llvm`) generates LLVM IR as text strings, writes a `.ll` file, and shells out to `llc` for compilation. This approach has zero LLVM dependencies but limits optimization — LLVM can't perform interprocedural analysis on textual IR passed through `llc` alone.

After fixing `-O3` passthrough, benchmarks show LLVM is already 13-36% faster than Cranelift on certain workloads. Migrating to the inkwell API (safe Rust bindings to LLVM C API) would enable the full LLVM optimization pipeline, potentially achieving 2-5x speedup over Cranelift.

## Goal

Replace the textual IR emitter in `kodo_codegen_llvm` with inkwell-based programmatic IR construction, enabling LLVM's full optimization pipeline (including inlining, loop vectorization, GVN, etc.) while maintaining the same MIR → native binary interface.

## Current Architecture

```
MIR Functions
    ↓
kodo_codegen_llvm::compile_module_to_llvm_ir()
    ↓
String (LLVM IR text)  ←── THIS IS THE BOTTLENECK
    ↓
std::fs::write("output.ll")
    ↓
llc -filetype=obj -O3 output.ll -o output.o
    ↓
cc output.o -lkodo_runtime -o binary
```

## Target Architecture

```
MIR Functions
    ↓
kodo_codegen_llvm::compile_module()
    ↓
inkwell::Module (in-memory LLVM IR)
    ↓
PassManager → optimization passes (O0/O1/O2/O3)
    ↓
TargetMachine::write_to_file() → output.o
    ↓
cc output.o -lkodo_runtime -o binary
```

## Key Changes

### 1. New Dependency

```toml
[dependencies]
inkwell = { version = "0.5", features = ["llvm18-0"] }
```

Requires LLVM 18 installed on the build system. The `llvm-sys` crate (inkwell's dependency) will link against the system LLVM.

### 2. Module Structure (unchanged)

The crate keeps the same module structure:
- `lib.rs` — entry point, module creation
- `function.rs` — function compilation
- `instruction.rs` — MIR instruction translation
- `value.rs` — value translation
- `builtins.rs` — runtime function declarations
- `types.rs` — type mapping

### 3. Key Type Mappings

| Current (String) | Inkwell |
|-----------------|---------|
| `"i64"` | `context.i64_type()` |
| `"double"` | `context.f64_type()` |
| `"void"` | `context.void_type()` |
| `"{ i64, i64 }"` | `context.struct_type(&[i64, i64], false)` |
| `format!("call ...")` | `builder.build_call(fn_val, &args, "result")` |
| `format!("add ...")` | `builder.build_int_add(lhs, rhs, "sum")` |

### 4. Emitter Replacement

Current `LLVMEmitter` builds strings:
```rust
emitter.indent("  %3 = add i64 %1, %2");
```

New approach uses inkwell builder:
```rust
let sum = builder.build_int_add(v1, v2, "sum")?;
```

### 5. Optimization Pipeline

```rust
let pass_manager = PassManager::create(());
pass_manager.add_instruction_combining_pass();
pass_manager.add_reassociate_pass();
pass_manager.add_gvn_pass();
pass_manager.add_cfg_simplification_pass();
pass_manager.add_basic_alias_analysis_pass();
pass_manager.add_promote_memory_to_register_pass();
pass_manager.add_function_inlining_pass();
pass_manager.add_loop_vectorize_pass();
pass_manager.run_on(&module);
```

### 6. Object File Emission

```rust
let target = Target::from_name("aarch64").unwrap();
let machine = target.create_target_machine(
    &TargetTriple::create("aarch64-apple-macosx"),
    "generic", "", OptimizationLevel::Aggressive,
    RelocMode::Default, CodeModel::Default,
)?;
machine.write_to_file(&module, FileType::Object, &obj_path)?;
```

No more shelling out to `llc`.

## Scope

### In Scope
- Replace textual IR emission with inkwell API calls
- Add optimization passes for -O0/-O1/-O2/-O3
- Direct object file emission (no llc dependency)
- Maintain all existing functionality (160+ builtins, structs, enums, closures, virtual calls)
- Keep `--emit-llvm` flag working (dump IR via `module.print_to_string()`)

### Out of Scope
- JIT compilation
- Debug info (DWARF)
- Cross-compilation
- LTO (link-time optimization)
- New optimizations beyond what LLVM provides

## Migration Strategy

### Phase 1: Infrastructure (2-3 days)
- Add inkwell dependency
- Create new `Context`, `Module`, `Builder` setup in lib.rs
- Implement type mapping (`types.rs`)
- Implement builtin declarations (`builtins.rs`)

### Phase 2: Core Translation (5-7 days)
- Implement value translation (`value.rs`)
- Implement instruction translation (`instruction.rs`)
  - Arithmetic, comparisons, casts
  - Memory operations (alloca, load, store)
  - Control flow (br, switch, phi)
  - Function calls (direct, indirect, virtual)
  - String operations (struct access)
- Implement terminator translation (`terminator.rs`)
- Implement function compilation (`function.rs`)

### Phase 3: Integration (2-3 days)
- Wire optimization passes
- Object file emission
- Update build.rs to use new API (remove llc dependency)
- Update `--emit-llvm` to use module.print_to_string()

### Phase 4: Validation (2-3 days)
- Run all 2700+ tests
- Run 60 UI tests
- Run 150+ examples
- Re-benchmark and document improvements
- Update performance.md with new numbers

## Expected Performance Improvement

| Benchmark | Current (textual) | Expected (inkwell) | Improvement |
|-----------|-------------------|-------------------|-------------|
| fib(35) | 0.26s | ~0.08-0.12s | 2-3x |
| sum 10M | 0.07s | ~0.01-0.03s | 2-7x |
| Compilation | 110ms (with llc) | ~80ms (direct) | 1.3x |

## Risks

1. **LLVM version coupling**: inkwell requires specific LLVM version installed
2. **Build complexity**: LLVM C++ dependency increases build time significantly
3. **Crate size**: inkwell + llvm-sys adds ~50MB to build artifacts
4. **Platform support**: LLVM must be available on all target platforms

## Verification

```bash
cargo test --workspace
make ui-test
make validate-everything
# Re-run performance benchmarks
./benchmarks/run_performance.sh
```
