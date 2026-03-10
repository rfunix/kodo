# Kōdo — Documento de Produto Completo

> **Uso deste documento:** Este é um prompt autossuficiente para ser usado em conversas com outras IAs. Contém toda a informação necessária para pesquisar, avaliar e sugerir próximos passos para o Kōdo.

---

## 1. O Que É o Kōdo?

**Kōdo** (コード, "código" em japonês) é uma **linguagem de programação compilada, de propósito geral**, projetada do zero para que **agentes de IA escrevam, raciocinem sobre e mantenham software** — mantendo total transparência e auditabilidade por humanos.

**Compilador:** Escrito em Rust, compila para binários nativos via Cranelift.

**Estágio:** MVP funcional. Programas com funções, contratos, loops, condicionais e chamadas compilam e executam nativamente.

---

## 2. Por Que o Kōdo Existe?

### O Problema

Agentes de IA geram código em linguagens existentes (Python, JavaScript, Rust, Go). Essas linguagens foram projetadas para **humanos**, com:

- **Ambiguidade sintática** — precedência implícita, coerções de tipo, "magia" que confunde agentes
- **Intenção invisível** — nada no código diz *por que* ele existe ou *o que deveria fazer*
- **Correção por teste, não por construção** — testes verificam depois; contratos poderiam verificar antes
- **Sem rastreabilidade** — impossível saber qual agente escreveu qual parte, com qual confiança
- **Sem auto-descrição** — binários compilados são caixas-pretas; sem como saber o que fazem sem código-fonte

### A Tese do Kōdo

> Se eliminarmos ambiguidade, tornarmos intenção explícita, embutirmos contratos na gramática, e tornarmos cada módulo auto-descritivo, agentes de IA produzem software **correto por construção** em vez de correto por teste.

O Kōdo não é uma linguagem para humanos que agentes podem usar. É uma linguagem **para agentes** que humanos podem auditar.

---

## 3. Diferenciadores Únicos

### 3.1 Zero Ambiguidade Sintática (LL(1))

Cada construção tem **exatamente uma interpretação válida**. A gramática é LL(1): um parser precisa olhar no máximo 1 token à frente para decidir que regra aplicar. Não há ambiguidades de precedência, dangling else, ou surpresas sintáticas.

- Sem conversões implícitas de tipo (nunca)
- Sem ponto-e-vírgula (blocos delimitados por `{}`)
- Sem coerções ou promoções automáticas
- Gramática formal EBNF completa e verificável

**Impacto:** Um agente de IA sabe *com certeza* o que qualquer trecho de código Kōdo significa. Zero chance de mal-interpretar.

### 3.2 Contratos como Cidadãos de Primeira Classe

Precondições (`requires`) e postcondições (`ensures`) são parte da gramática, não comentários:

```kodo
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
    ensures { result >= 0 }
{
    return a / b
}
```

- `requires` é verificado em runtime antes da execução
- `ensures` é verificado antes de cada `return`
- Violações abortam com mensagem clara
- **Validadores automáticos:** Para cada função com `requires`, o compilador gera uma função `validate_nome()` que avalia as precondições sem efeitos colaterais, permitindo que agentes validem entradas antes de chamar a função

**Planejado:** Verificação estática via Z3 SMT solver quando decidível.

**Impacto:** Agentes expressam invariantes formais. O compilador garante que são respeitados. Software é correto por construção.

### 3.3 Módulos Auto-Descritivos (Meta Obrigatório)

Todo módulo Kōdo **deve** ter um bloco `meta` com pelo menos `purpose`:

```kodo
module payment_processor {
    meta {
        purpose: "Processa pagamentos via gateway externo",
        version: "2.1.0",
        author: "agent:claude-4"
    }
    // ...
}
```

O compilador **rejeita** módulos sem `meta` ou sem `purpose`. Isso não é opcional.

**Impacto:** Qualquer pessoa ou agente que encontre um módulo Kōdo sabe imediatamente o que ele faz, por que existe, e quem o escreveu.

### 3.4 Trust Chains — Rastreabilidade de Autoria

Anotações de primeira classe para rastrear autoria e confiança:

```kodo
@authored_by("agent:claude-4")
@confidence(95)
@reviewed_by("human:alice")
fn transfer_funds(amount: Int, to: String) -> Bool
    requires { amount > 0 }
{ ... }
```

O compilador pode **enforçar políticas de confiança**:
- Se `trust_policy: "high_security"` está no meta, **toda** função deve ter `@authored_by`
- Funções com `@confidence` abaixo de 85% **devem** ter `@reviewed_by` com revisor humano
- Violações são erros de compilação (não warnings)

**Impacto:** Cadeia de custódia completa. Sabemos quem escreveu, com qual confiança, e se um humano revisou. Nenhuma outra linguagem faz isso.

### 3.5 Certificados de Compilação

Cada `kodoc build` emite um `.ko.cert.json` junto ao binário:

```json
{
  "module": "payment_processor",
  "purpose": "Processa pagamentos via gateway externo",
  "version": "2.1.0",
  "compiled_at": "2026-03-09T14:22:00Z",
  "compiler_version": "0.1.0",
  "contracts": {
    "requires_count": 5,
    "ensures_count": 3,
    "mode": "runtime"
  },
  "functions": ["transfer_funds", "validate_payment", "main"],
  "validators": ["validate_transfer_funds", "validate_validate_payment"],
  "source_hash": "sha256:abc123...",
  "binary_hash": "sha256:def456...",
  "certificate_hash": "sha256:789abc...",
  "parent_certificate": "sha256:previous...",
  "diff_from_parent": {
    "functions_added": ["validate_payment"],
    "functions_removed": [],
    "contracts_changed": true,
    "source_hash_changed": true
  }
}
```

Certificados são **encadeados**: cada compilação referencia a anterior, criando uma cadeia de proveniência verificável. Agentes de deploy podem rastrear toda a história de compilações.

**Impacto:** Proveniência end-to-end. De quem escreveu até o binário em produção, com hashes verificáveis.

### 3.6 Binários Auto-Explicativos

Binários compilados com Kōdo respondem a `--describe`:

```bash
$ ./payment_processor --describe
{
  "module": "payment_processor",
  "purpose": "Processa pagamentos via gateway externo",
  "functions": [
    {
      "name": "transfer_funds",
      "params": [{"name": "amount", "type": "Int"}, {"name": "to", "type": "String"}],
      "return_type": "Bool",
      "requires": ["requires clause 1"],
      "annotations": {"authored_by": "agent:claude-4", "confidence": 95}
    }
  ],
  "validators": ["validate_transfer_funds"]
}
```

Agentes descobrem o que um binário faz **sem código fonte**. Metadados são embarcados no executável.

**Impacto:** Um agente de deploy pode inspecionar qualquer binário Kōdo e entender suas capacidades, contratos e autoria.

### 3.7 Saída Estruturada para Agentes (`--json-errors`)

Erros de compilação são emitidos em JSON estruturado:

```json
{
  "errors": [{
    "code": "E0200",
    "severity": "error",
    "message": "type mismatch: expected `Int`, found `String`",
    "span": {"file": "src/main.ko", "start_line": 10, "start_col": 5}
  }],
  "status": "failed",
  "meta": {"module": "payment", "purpose": "..."}
}
```

Cada erro tem código único, localização precisa, e o campo meta do módulo. Agentes parsam JSON em vez de tentar interpretar mensagens de texto.

**Impacto:** Feedback loop ultra-rápido. Agente lê erro → corrige → recompila, sem ambiguidade na interpretação de erros.

### 3.8 Sistema de Intent (Planejado)

A feature mais ambiciosa. Agentes declaram **o que** querem, o compilador gera **como**:

```kodo
intent serve_http {
    port: 8080
    routes: [
        GET "/greet/:name" => handle_greet,
        POST "/users" => create_user
    ]
}
```

O resolver built-in geraria: servidor HTTP, routing, middleware, pools de conexão — tudo verificado contra os contratos das funções handler.

**Status:** Parsing existe, resolução não está implementada. É o próximo grande passo.

---

## 4. Arquitetura do Compilador

```
Source (.ko)
    │
    ▼
[kodo_lexer]     → Token stream (logos)
    │
    ▼
[kodo_parser]    → AST (recursive descent LL(1), hand-written)
    │
    ▼
[kodo_types]     → Typed AST (type checking, sem inference cross-module)
    │
    ▼
[kodo_contracts] → Verified AST (runtime checks; Z3 SMT planejado)
    │
    ▼
[kodo_resolver]  → Expanded AST (intents → código concreto; stub)
    │
    ▼
[kodo_mir]       → Mid-level IR (CFG com basic blocks e terminators)
    │
    ▼
[kodo_codegen]   → Native binary (Cranelift → Mach-O/ELF)
    │
    ▼
[kodo_runtime]   → Linked staticlib (entry point, builtins, --describe)
```

**Workspace Rust com 11 crates**, sem dependências circulares. ~230 testes, zero clippy warnings.

---

## 5. O Que Funciona Hoje (MVP)

### Funcionando end-to-end (source → binary executável):
- Funções com parâmetros tipados e tipos de retorno
- Tipos: `Int`, `Bool`, `String` (literais)
- Operadores: aritméticos, comparação, lógicos, unários
- `if`/`else`, `return`, `while` loops
- Variáveis: `let` (imutável), `let mut` (mutável), reassignment
- Recursão e chamadas entre funções
- Contratos `requires` e `ensures` com verificação em runtime
- Builtins: `println`, `print`, `print_int`
- Certificados de compilação encadeados
- Binários auto-explicativos (`--describe`)
- Trust chains com enforcement de políticas
- Validadores automáticos (`validate_fn_name()`)
- JSON error output estruturado

### Parcialmente implementado:
- Anotações (`@name(args)`) — parsing completo, enforcement via trust policy
- Intent blocks — parsing como stub, resolver não implementado

### Planejado mas não implementado:
- Structs e enums (tipos customizados)
- Generics
- Traits
- Ownership system (`own`/`ref`/`mut` além de `mut` para variáveis)
- Z3 SMT solver para verificação estática de contratos
- Pattern matching
- Closures e higher-order functions
- Multi-file compilation e imports
- Standard library
- Otimizações de MIR (SSA, DCE, constant folding)
- LLVM backend para builds otimizadas
- Intent resolver strategies

---

## 6. Fundações Acadêmicas

O design do Kōdo é fundamentado em teoria de compiladores e linguagens estabelecida:

| Área | Referências |
|------|------------|
| Lexer | *Crafting Interpreters* Ch.4, *Engineering a Compiler* Ch.2 |
| Parser | *Crafting Interpreters* Ch.6-8, *Engineering a Compiler* Ch.3 |
| Type Safety | *Types and Programming Languages* (Pierce) Ch.1-11 |
| Ownership | *Advanced Topics in Types and PL* Ch.1 (linear/affine types) |
| Contratos | *Software Foundations* Vol.1-2, *Calculus of Computation* Ch.1-6 |
| SMT Verification | *Calculus of Computation* Ch.10-12 |
| MIR/Codegen | *Modern Compiler Implementation in ML* Ch.7-11 |

---

## 7. Linguagem — Referência Rápida

```kodo
module nome_do_modulo {
    meta {
        purpose: "O que este módulo faz",
        version: "1.0.0",
        author: "quem escreveu"
    }

    @authored_by("agent:claude-4")
    @confidence(95)
    fn nome_funcao(param: Tipo, param2: Tipo2) -> TipoRetorno
        requires { precondição_booleana }
        ensures { postcondição_usando_result }
    {
        let x: Int = 42
        let mut counter: Int = 0
        while counter < 10 {
            counter = counter + 1
        }
        if x > 0 {
            return x
        }
        return 0
    }
}
```

**Tipos primitivos:** `Int`, `Int8`-`Int64`, `Uint`, `Uint8`-`Uint64`, `Float32`, `Float64`, `Bool`, `String`, `Byte`

**Filosofia:** Sem null (Option), sem exceções (Result), sem conversões implícitas, sem ambiguidade.

---

## 8. Stack Técnico

- **Linguagem do compilador:** Rust
- **Lexer:** logos 0.16.1
- **Code generation:** Cranelift 0.129.1
- **Error reporting:** ariadne 0.6.0, thiserror
- **Hashing:** sha2
- **Serialização:** serde, serde_json
- **SMT (planejado):** z3 0.19.13
- **Testing:** insta (snapshots), proptest (property-based), criterion (benchmarks)
- **Plataformas:** macOS (ARM64), Linux (x86_64)

---

## 9. Posicionamento Competitivo

| Característica | Kōdo | Rust | Go | TypeScript | Dafny | Whiley |
|---|---|---|---|---|---|---|
| Projetado para agentes IA | **Sim** | Não | Não | Não | Não | Não |
| Contratos na gramática | **Sim** | Não | Não | Não | Sim | Sim |
| Meta obrigatório | **Sim** | Não | Não | Não | Não | Não |
| Trust chains / autoria | **Sim** | Não | Não | Não | Não | Não |
| Certificados de compilação | **Sim** | Não | Não | Não | Não | Não |
| Binários auto-explicativos | **Sim** | Não | Não | N/A | Não | Não |
| JSON errors para agentes | **Sim** | Parcial | Não | Parcial | Não | Não |
| Zero ambiguidade (LL(1)) | **Sim** | Não | Parcial | Não | Não | Não |
| Validadores automáticos | **Sim** | Não | Não | Não | Parcial | Parcial |
| Intent system | **Planejado** | Não | Não | Não | Não | Não |

**Linguagens mais próximas conceitualmente:**
- **Dafny** (Microsoft) — verificação formal, mas não focada em agentes
- **Whiley** — contratos como cidadãos de primeira classe, mas acadêmico
- **Ada/SPARK** — contratos e verificação, mas para embedded/aeroespacial

**Diferencial absoluto:** Nenhuma linguagem existente combina: (1) design para agentes, (2) contratos na gramática, (3) certificados de compilação, (4) trust chains, e (5) binários auto-explicativos.

---

## 10. Métricas do Projeto

- **~230 testes** (unit, snapshot, property-based)
- **Zero clippy warnings** com pedantic mode
- **11 crates** no workspace
- **7 exemplos** que compilam e executam
- **Pipeline completo** source → token → AST → typed AST → MIR → native binary
- **4 programas de exemplo** executando end-to-end (hello, fibonacci, while_loop, contracts_demo)

---

## 11. Perguntas para Pesquisa (Sugestões de Prompt)

Use este documento como contexto e explore:

1. **Próximas features:** "Dado o estado atual do Kōdo (MVP com contratos runtime, certificados, trust chains), quais são as próximas features mais impactantes para adoção por agentes de IA?"

2. **Intent system:** "Como implementar um sistema de resolução de intenções (intent → código concreto) em um compilador, onde blocos declarativos são mapeados para implementações verificadas contra contratos?"

3. **Mercado:** "Existe mercado para uma linguagem de programação projetada especificamente para agentes de IA? Quem seriam os primeiros adotantes?"

4. **SMT integration:** "Como integrar Z3 SMT solver em um compilador para verificar precondições/postcondições estaticamente, com fallback para runtime?"

5. **Ecossistema:** "Que tipo de ecossistema (package manager, LSP, CI/CD integrations, IDE plugins) uma linguagem agent-first precisa para ser viável?"

6. **Comparação acadêmica:** "Como o Kōdo se compara com Dafny, Whiley, Ada/SPARK e outras linguagens com verificação formal? Quais lições aprender?"

7. **Modelo de negócio:** "Quais modelos de negócio são viáveis para uma linguagem de programação open-source focada em agentes de IA?"

---

*Documento gerado em 2026-03-09. Reflete o estado real do compilador com ~230 testes passando.*
