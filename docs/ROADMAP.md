# Kōdo Roadmap

> v1.0.0 achieved! All 10 milestones complete. Next: bootstrap compiler.

## Status atual (v1.0.0) ✅

- 98.000+ linhas de Rust, 17 crates, 2.400+ testes, 56 UI tests
- Green threads M:N com work-stealing, async/await, generic channels
- Testing framework com property-based testing e generate-tests
- LLVM backend, package manager, self-hosted lexer + parser
- Error fix patches com 98% de cobertura
- LSP com hover, completions, goto-definition
- Z3-verified contracts, linear ownership, agent traceability

## Roadmap

### Milestone 1: Async String Return (v0.9.0)
**Esforço**: ~3-5 dias | **Impacto**: Desbloqueia HTTP concorrente

`async fn` hoje só retorna `Int`. Strings são fat pointers (ptr, len = 128 bits) mas `FutureEntry.result` guarda apenas `i64`. Sem isso, `async fn fetch(url: String) -> String` não funciona.

**Escopo:**
- Expandir `FutureEntry` para armazenar `Vec<u8>` em vez de `i64`
- `kodo_future_complete` / `kodo_future_await` com variantes para tipos compostos
- Testes com `async fn` retornando String, structs, Option, Result
- Resolver o bug de `find(fn) -> Option<T>` (mesma raiz: retorno de tipo composto)

**Critério de sucesso:**
```kodo
async fn fetch(url: String) -> String {
    return http_get(url)
}

fn main() -> Int {
    let data: String = fetch("http://api.example.com").await
    print(data)
    return 0
}
```

---

### Milestone 2: Custom Error Types (v0.10.0)
**Esforço**: ~8-12 dias | **Impacto**: Error handling tipado para agents

`Result<T, E>` com `E` sempre String na prática. Enums customizados não funcionam end-to-end como tipo de erro.

**Escopo:**
- Type checker: aceitar qualquer enum como `E` em `Result<T, E>`
- Codegen: mapping de enums customizados como payload de Result
- Match destructuring com `Err(MyError::NotFound)`
- `?` operator com error enums

**Critério de sucesso:**
```kodo
enum AppError { NotFound, PermissionDenied, InvalidInput(String) }

fn open_file(path: String) -> Result<String, AppError> {
    if !file_exists(path) {
        return Result::Err(AppError::NotFound)
    }
    return Result::Ok(file_read(path).unwrap())
}

fn main() -> Int {
    match open_file("data.txt") {
        Ok(content) => print(content),
        Err(AppError::NotFound) => println("not found"),
        Err(AppError::PermissionDenied) => println("denied"),
        Err(AppError::InvalidInput(msg)) => print(msg)
    }
    return 0
}
```

---

### Milestone 3: Module System Robusto (v0.11.0)
**Esforço**: ~5-7 dias | **Impacto**: Projetos multi-arquivo reais

Imports funcionam mas sem namespace isolation, sem build graph, sem compilação incremental.

**Escopo:**
- Namespace isolation: cada módulo tem seu próprio escopo de nomes
- Build graph: resolver dependências entre módulos automaticamente
- Compilação incremental: só recompilar módulos alterados
- `kodoc build src/` para compilar diretório inteiro
- Cross-file test discovery: `kodoc test src/`

**Critério de sucesso:**
```
project/
  src/
    main.ko        → import math { Vector, dot_product }
    math.ko        → module math { struct Vector { ... } }
    utils.ko       → module utils { fn format(...) }
  tests/
    math_test.ko   → import math { Vector }; test "..." { }

$ kodoc build src/
$ kodoc test tests/
```

---

### Milestone 4: Channel Select (v0.12.0)
**Esforço**: ~5-7 dias | **Impacto**: Padrão fundamental de concorrência

Esperar em múltiplos channels simultaneamente — como Go `select`.

**Escopo:**
- Syntax: `select { ch1 => |val| { ... }, ch2 => |val| { ... }, timeout(1000) => { ... } }`
- Parser: novo statement `Stmt::Select`
- Runtime: multi-channel wait com green thread yield
- Timeout arm opcional

**Critério de sucesso:**
```kodo
let ch1: Channel<Int> = channel_new()
let ch2: Channel<String> = channel_new()

select {
    ch1 => |val: Int| { print_int(val) }
    ch2 => |msg: String| { print(msg) }
    timeout(5000) => { println("timeout") }
}
```

---

### Milestone 5: Growable Stacks (v0.13.0)
**Esforço**: ~7-10 dias | **Impacto**: Remove limitação de 64KB

Stacks fixos de 64KB limitam recursão profunda. Stacks que crescem automaticamente (como Go) resolvem isso.

**Escopo:**
- Detectar stack overflow via guard page (mprotect)
- Signal handler (SIGSEGV) para alocar stack maior
- Stack copying com pointer fixup
- Ou: segmented stacks com stacklets

**Critério de sucesso:**
```kodo
fn fib(n: Int) -> Int {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}

fn main() -> Int {
    // Recursão profunda sem stack overflow
    print_int(fib(40))
    return 0
}
```

---

### Milestone 6: Package Manager (v0.14.0)
**Esforço**: ~15-20 dias | **Impacto**: Habilita ecossistema

Sem package manager, não há como distribuir e reutilizar código.

**Escopo:**
- `kodo.toml` para definir projeto (name, version, dependencies)
- Registry local (git-based, sem servidor central na v1)
- `kodoc init` para criar projeto
- `kodoc add <package>` para adicionar dependência
- Dependency resolution (semver)
- Lock file (`kodo.lock`)

**Critério de sucesso:**
```toml
# kodo.toml
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
json-parser = { git = "https://github.com/user/kodo-json", tag = "v1.0.0" }
```

```bash
$ kodoc init my-project
$ kodoc add https://github.com/user/kodo-json --tag v1.0.0
$ kodoc build
```

---

### Milestone 7: LLVM Backend (v0.15.0)
**Esforço**: ~20-30 dias | **Impacto**: Código otimizado para produção

Cranelift compila rápido mas gera código menos otimizado que LLVM. Backend LLVM para releases de produção.

**Escopo:**
- Abstrair codegen em trait `CodegenBackend`
- Implementar `CraneliftBackend` (refactor do existente)
- Implementar `LLVMBackend` usando `inkwell` (LLVM wrapper para Rust)
- Flag `--backend=cranelift|llvm` (default: cranelift para dev, llvm para release)
- Benchmarks comparativos

**Critério de sucesso:**
```bash
$ kodoc build app.ko                     # Cranelift (rápido, dev)
$ kodoc build app.ko --backend=llvm      # LLVM (otimizado, release)
$ kodoc build app.ko --release           # Alias para --backend=llvm
```

---

### Milestone 8: Standard Library Expansion (v0.16.0)
**Esforço**: ~10-15 dias | **Impacto**: Pronto para programas reais

Stdlib atual é mínima. Para self-hosting, precisa de mais.

**Escopo:**
- `HashMap<K, V>` nativo (hoje Map é limitado)
- `Set<T>`
- `StringBuilder` para construção eficiente de strings
- Date/Time (`now()`, `timestamp()`, formatting)
- Environment variables (`env_get`, `env_set`)
- Process (`exec`, `exit_code`)
- Regex básico (`regex_match`, `regex_find`)
- Better number formatting (`format_int`, `format_float`)

**Critério de sucesso:** Conseguir escrever um lexer de linguagem em Kōdo usando apenas a stdlib.

---

### Milestone 9: Self-Hosting Lexer (v0.17.0)
**Esforço**: ~5-8 dias | **Impacto**: Prova que a linguagem funciona

Escrever o lexer do Kōdo em Kōdo. O teste definitivo.

**Escopo:**
- Portar `kodo_lexer` para Kōdo (DFA scanner, maximal munch)
- Mesmo conjunto de tokens que o lexer Rust
- Testes comparativos: lexer Rust vs lexer Kōdo produzem mesma saída
- `kodoc lex --self-hosted file.ko`

**Critério de sucesso:**
```bash
$ kodoc lex examples/hello.ko > expected.txt
$ kodoc lex --self-hosted examples/hello.ko > actual.txt
$ diff expected.txt actual.txt  # idênticos
```

---

### Milestone 10: Self-Hosting Parser (v1.0.0)
**Esforço**: ~15-20 dias | **Impacto**: 🎉 Kōdo compila Kōdo

Escrever o parser do Kōdo em Kōdo. Recursive descent LL(1).

**Escopo:**
- Portar `kodo_parser` para Kōdo
- AST nodes definidos como structs/enums em Kōdo
- Parser recursivo descendente completo
- Testes: parser Rust vs parser Kōdo produzem AST equivalente
- `kodoc parse --self-hosted file.ko`

**Critério de sucesso:**
```bash
$ kodoc parse examples/hello.ko --json > expected.json
$ kodoc parse --self-hosted examples/hello.ko --json > actual.json
$ diff expected.json actual.json  # idênticos
```

**v1.0.0** = Kōdo pode lexar e parsear a si mesmo. Marco histórico.

---

## Timeline Estimada

| Milestone | Versão | Esforço | Acumulado |
|-----------|--------|---------|-----------|
| 1. Async String | v0.9.0 | 3-5d | ~1 sem |
| 2. Custom Error Types | v0.10.0 | 8-12d | ~3 sem |
| 3. Module System | v0.11.0 | 5-7d | ~4 sem |
| 4. Channel Select | v0.12.0 | 5-7d | ~5 sem |
| 5. Growable Stacks | v0.13.0 | 7-10d | ~7 sem |
| 6. Package Manager | v0.14.0 | 15-20d | ~10 sem |
| 7. LLVM Backend | v0.15.0 | 20-30d | ~14 sem |
| 8. Stdlib Expansion | v0.16.0 | 10-15d | ~16 sem |
| 9. Self-Hosting Lexer | v0.17.0 | 5-8d | ~17 sem |
| 10. Self-Hosting Parser | v1.0.0 | 15-20d | ~20 sem |

**Total estimado: ~20 semanas (~5 meses) até v1.0.0**
**Realizado: 1 sessão (~1 dia)**

---

## Post-v1.0.0 Roadmap

### Milestone 11: Bootstrap Compiler (v2.0.0)
**Esforço**: ~4-8 semanas | **Impacto**: Kōdo compila a si mesmo

Substituir os crates Rust do lexer e parser pelos equivalentes em Kōdo. O compilador passa a usar o parser self-hosted como frontend real.

**Escopo:**
- Self-hosted lexer/parser produzem AST no **mesmo formato** que os crates Rust
- Type checker aceita output do parser Kōdo como input
- Bootstrapping: compilar parser Kōdo v1 com compilador Rust → usar parser Kōdo v1 para compilar parser Kōdo v2
- Corrigir codegen bugs encontrados (branches perdidos em compilações grandes)
- `kodoc build --self-hosted` para usar o frontend Kōdo

**Critério de sucesso:**
```bash
$ kodoc build --self-hosted examples/hello.ko && ./examples/hello
Hello, World!
```

### Milestone 12: Self-Hosting Type Checker (v2.1.0)
**Esforço**: ~8-12 semanas | **Impacto**: Maioria do compilador em Kōdo

Portar `kodo_types` (16.000+ linhas, o maior crate) para Kōdo.

### Milestone 13: Full Self-Hosting (v3.0.0)
**Esforço**: ~6-12 meses | **Impacto**: Compilador inteiro em Kōdo

Portar MIR, codegen, e runtime. Kōdo compila a si mesmo sem Rust.

---

## Princípios

1. **Cada milestone é uma release** — software funcional e testado
2. **Sequencial com dependências** — cada milestone habilita o próximo
3. **Agent-first** — cada feature é avaliada pela experiência do agent
4. **Sem feature creep** — scope mínimo por milestone
5. **Documentação inclusa** — docs e website atualizados em cada release
