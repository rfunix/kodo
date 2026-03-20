# Kōdo Language Website — Design & Documentation

## O que é Kōdo?

Kōdo (コード) é uma linguagem de programação compilada, projetada para **AI agents escreverem, raciocinarem e manterem software** — enquanto permanece totalmente transparente e auditável por humanos.

**Tese central**: Remover ambiguidade, tornar intent explícito, embutir contratos na gramática, tornar cada módulo auto-descritivo. AI agents produzem software correto por construção.

**Não é uma toy language.** Kōdo tem: zero ambiguidade sintática (LL(1)), contratos first-class (requires/ensures verificados por Z3 SMT solver), módulos auto-descritivos (meta obrigatório), intent-driven programming (blocos intent), ownership linear (own/ref/mut), concorrência estruturada, e anotações de rastreabilidade de agentes (@authored_by, @confidence, @reviewed_by).

**Versão atual**: v0.5.0 | **Testes**: 2249 passando | **Crates Rust**: 13

## Tech Stack Atual

- **Astro 6.0.1** — Static site generator
- **Starlight 0.38.1** — Framework de documentação (sidebar, search, dark mode)
- **Tailwind CSS 4.2.1** — Styling
- **Shiki + TextMate grammar customizado** — Syntax highlighting para `.ko`
- **Pagefind** — Client-side search
- **Tipografia**: Space Grotesk (display), JetBrains Mono (code)

## Design System Atual

### Cores
- **Accent**: Indigo (#4f46e5 principal, #1e1b4b dark, #a5b4fc light)
- **Background**: Slate escuro (#17181c base)
- **Code syntax**: violet-400 (keywords), blue-300 (identifiers), cyan-300 (types), amber-300 (strings), orange-300 (números), emerald-400 (annotations), slate-500 (comments)

### Estilo Visual
- Dark-first design
- Borders: slate-800, indigo-500/30 on hover
- Cards com rounded-xl, backdrop-blur
- Scroll-triggered animations (fade-in + slide-up)
- Grid patterns sutis como background overlay

## Estrutura do Site Atual

### Landing Page (/)
1. **Hero** — Tagline + CTA + code preview com syntax highlighting
2. **Problem** — 6 problemas expandíveis com soluções (click-to-reveal)
3. **CodeShowcase** — 4 tabs interativas: Contracts, Agent Traceability, Intents, Testing
4. **TrustEnforcement** — 3 colunas: Compilation Blocking, Transitive Propagation, Contract Status
5. **ClosedLoop** — Diagrama horizontal: write → kodoc check --json → kodoc fix → build succeeds
6. **Features** — Grid 3 colunas com 6 features
7. **RealWorldExamples** — Exemplos de código
8. **QuickStart** — 3 steps: install, write hello.ko, build & run
9. **Stats** — Estatísticas do projeto
10. **Footer**

### Documentação (/docs/)
Via Starlight com sidebar. 28+ páginas organizadas em:

**Getting Started**: Installation, Tour

**Language Guide** (13 páginas):
- Core: Language Basics, Data Types, Pattern Matching, Error Handling
- Advanced: Ownership, Generics, Traits, Methods, Closures, Iterators
- Special: String Interpolation, Modules & Imports, Functional Programming

**Advanced Topics** (8 páginas):
- Contracts, Agent Traceability, Concurrency, Actors, HTTP & Networking, MCP Server, Testing, Real-World Examples

**Tools** (2 páginas): CLI Reference, CLI Tools

**Standard Library**: Stdlib Reference

**Reference** (5 páginas): Language Design, Grammar (EBNF), Error Index, Why not X?, Benchmarks

## Killer Features para Destacar no Design

### 1. Contracts Verificados por Z3
```kodo
fn divide(a: Int, b: Int) -> Int
    requires { b != 0 }
    ensures  { result * b == a }
{
    return a / b
}
```
O compilador usa Z3 SMT solver para verificar contratos estaticamente. Se não consegue provar, insere verificação runtime. Nenhuma outra linguagem oferece isso nativamente para agents.

### 2. Agent Traceability & Confidence
```kodo
@authored_by(agent: "claude-4", task: "PROJ-42")
@confidence(0.95)
fn process_payment(amount: Int) -> Result<String, String>
    requires { amount > 0 }
{
    // Se confidence < 0.8, compilador BLOQUEIA até @reviewed_by
    return Ok("processed")
}

@reviewed_by(human: "rafael", date: "2026-03-19")
fn critical_operation() -> Void { ... }
```
Propagação transitiva: se A (0.95) chama B (0.5), o score efetivo de A cai para 0.5. Compilation certificates (.ko.cert.json) persistem scores e status de contratos.

### 3. Closed-Loop Repair Cycle (THE killer UX)
```
Agent writes code → kodoc check --json-errors → structured errors with fix patches
→ kodoc fix (auto-applies patches) → kodoc build → binary
```
Errors são machine-parseable JSON com byte offsets para auto-fix. O compilador é literalmente designed para agents consertarem código sem intervenção humana.

### 4. Intent-Driven Programming
```kodo
module api {
    meta { description: "REST API service" }

    intent http_server {
        port: 8080
        route "/health" -> health_check
        route "/users" -> get_users
    }

    fn health_check() -> String { return "ok" }
    fn get_users() -> String { return "[{\"name\": \"Alice\"}]" }
}
```
Agents declaram O QUE querem, compilador gera O COMO. 12 resolvers built-in (http_server, database, json_api, cache, queue, worker, etc).

### 5. Linear Ownership System
```kodo
fn transfer(own account: Account) -> Account {
    // 'own' = proprietário único, move semantics
    // 'ref' = empréstimo imutável
    // 'mut' = empréstimo mutável exclusivo
    return account  // ownership transferida
}
```

### 6. Self-Describing Modules
```kodo
module payment_service {
    meta {
        description: "Handles all payment processing"
        version: "2.1.0"
        author: "claude-4"
        min_confidence: 0.9
    }
    // Compilador EXIGE meta block — módulos são auto-documentados
}
```

## O que Quero que Você Faça

### Redesign da Landing Page

Redesenhe a landing page mantendo a identidade visual (dark theme, indigo accent, Space Grotesk + JetBrains Mono) mas melhorando:

1. **Hero Section** — Mais impactante. Mostrar um code snippet real de Kōdo com annotations visíveis. Tagline que comunica "A programming language where AI agents write correct software by construction". Dois CTAs: "Get Started" e "Try in Playground".

2. **"Why Kōdo?" Section** — Substituir o "Problem" atual por uma narrativa mais fluida. Comparar lado-a-lado: "Without Kōdo" (código genérico sem garantias) vs "With Kōdo" (contracts, confidence, auto-fix). Visual comparison, não accordion.

3. **Feature Showcase** — Redesenhar as tabs de código. Cada feature deve ter:
   - Título + subtítulo de 1 linha
   - Code snippet real compilável
   - Annotation visual do que o compilador faz (setas, highlights)
   - "Try it" link para a doc correspondente

4. **Closed-Loop Diagram** — Redesenhar como um flow diagram animado/interativo mostrando o ciclo: Write → Check → Error (JSON) → Fix → Build → Deploy. Destacar que agents operam neste loop autonomamente.

5. **Trust Pipeline** — Visualização mais sofisticada da propagação de confidence. Mostrar um grafo de chamadas onde confidence flui transitivamente, com nós coloridos por score.

6. **Real-World Examples** — Cards clicáveis mostrando 4-5 exemplos reais:
   - Todo App (demonstra ownership + contracts)
   - HTTP API (demonstra intents + JSON)
   - Config Validator (demonstra error handling + contracts)
   - Audit Log (demonstra multi-file + traceability)
   - Health Checker (demonstra collections + functional)

7. **Ecosystem Section** (NOVA) — Mostrar as ferramentas:
   - REPL interativo
   - VSCode Extension
   - LSP Server
   - MCP Server (para AI agents nativamente)
   - CLI completo (lex, parse, check, build, fix, explain, audit)

8. **Stats Section** — Atualizar com números reais:
   - 2249 testes passando
   - 122 exemplos compiláveis
   - 13 crates no workspace
   - 28+ páginas de documentação
   - v0.5.0 com 6 releases

9. **QuickStart** — Manter os 3 steps mas com melhor visual

10. **Footer** — Links para docs, GitHub, releases, error index

### Redesign das Docs

Manter Starlight como framework mas melhorar a experiência:

1. **Docs Landing** (`/docs/`) — Redesenhar com learning paths visuais:
   - **Path 1: "I'm new"** → Tour → Getting Started → Language Basics → Data Types → First Program
   - **Path 2: "I know programming"** → Tour → Ownership → Contracts → Agent Traceability → Intents
   - **Path 3: "I'm building with agents"** → Agent Traceability → MCP Server → CLI Tools → Closed-Loop Guide
   - **Path 4: "Reference"** → Stdlib → Error Index → Grammar → CLI Reference

2. **Sidebar Reorganizada**:
   ```
   📚 Getting Started
     → Installation
     → Language Tour
     → Your First Program

   📖 Language Guide
     → Language Basics
     → Data Types & Structs
     → Pattern Matching
     → Error Handling (Option/Result)
     → String Interpolation
     → Modules & Imports

   🔧 Advanced Features
     → Ownership (own/ref/mut)
     → Generics & Type Bounds
     → Traits & Methods
     → Closures & Higher-Order Functions
     → Iterators & Functional
     → Contracts (requires/ensures)

   🤖 Agent-First Features
     → Agent Traceability (@confidence, @authored_by)
     → Intent System
     → Closed-Loop Repair
     → MCP Server Integration
     → Compilation Certificates

   ⚡ Concurrency
     → Spawn & Tasks
     → Actors
     → Channels

   🌐 Networking & I/O
     → HTTP Client & Server
     → File I/O
     → CLI Tools
     → JSON Processing

   🧪 Testing
     → Test Framework
     → Real-World Examples

   📋 Reference
     → Standard Library
     → CLI Reference
     → Error Code Index (E0001-E0699)
     → Grammar (EBNF)
     → Language Design Philosophy
     → Why not X? (Comparisons)
     → Benchmarks
   ```

3. **Cada página de doc deve ter**:
   - Breadcrumbs claros
   - "On this page" sidebar à direita
   - Code examples com syntax highlighting (já temos TextMate grammar)
   - "Try it" boxes mostrando como compilar o exemplo
   - Links "Next" e "Previous" no bottom
   - Edit on GitHub link

4. **Error Index Page** — Redesenhar como searchable/filterable:
   - Filtro por phase (Lexer, Parser, Types, Contracts, Resolver, MIR, Codegen)
   - Cada erro com: código, título, descrição, exemplo de código que causa, fix sugerido
   - Link para `kodoc explain <code>`

5. **Stdlib Reference** — Redesenhar com melhor organização:
   - Módulos: core, collections, string, io, http, json, math, cli, concurrency
   - Cada função com: signature, description, example, return type
   - Searchable

## Syntax da Linguagem (para code examples)

```kodo
// Módulo com meta obrigatório
module example {
    meta {
        description: "Example module"
        version: "1.0.0"
    }

    // Tipos primitivos
    let x: Int = 42
    let name: String = "Kōdo"
    let active: Bool = true
    let pi: Float64 = 3.14159

    // Structs
    struct Point {
        x: Int,
        y: Int
    }

    // Enums com pattern matching
    enum Shape {
        Circle(Int),
        Rectangle(Int, Int)
    }

    fn area(shape: Shape) -> Int {
        match shape {
            Shape.Circle(r) => r * r * 3,
            Shape.Rectangle(w, h) => w * h
        }
    }

    // Generics
    fn max<T: Ord>(a: T, b: T) -> T {
        if a > b { return a }
        return b
    }

    // Option e Result (sem null, sem exceptions)
    fn find(id: Int) -> Option<String> {
        if id == 1 { return Some("found") }
        return None
    }

    fn parse(s: String) -> Result<Int, String> {
        let n: Int = s.parse_int()
        if n < 0 { return Err("negative") }
        return Ok(n)
    }

    // Contracts
    fn sqrt(n: Float64) -> Float64
        requires { n >= 0.0 }
        ensures  { result >= 0.0 }
    {
        return math_sqrt(n)
    }

    // F-strings
    let greeting: String = f"Hello, {name}! x = {x}"

    // Collections
    let items: List<Int> = [1, 2, 3, 4, 5]
    let config: Map<String, String> = {"key": "value"}

    // Closures
    let doubled: List<Int> = items.map(fn(x: Int) -> Int { return x * 2 })

    // For-in loops
    for item in items {
        println(f"Item: {item}")
    }

    // Annotations de agente
    @authored_by(agent: "claude-4")
    @confidence(0.92)
    fn important_logic() -> Int {
        return 42
    }

    // Intent blocks
    intent http_server {
        port: 3000
        route "/api" -> handler
    }

    // Testes inline
    test "basic math" {
        assert_eq(2 + 2, 4)
    }
}
```

## Restrições

1. **Manter Astro + Starlight + Tailwind** — não trocar de framework
2. **Manter o dark theme como default** com light theme funcional
3. **Manter Space Grotesk + JetBrains Mono**
4. **Manter a paleta indigo/slate** mas pode refinar
5. **Syntax highlight** deve usar o TextMate grammar customizado existente
6. **Performance**: zero JS desnecessário, Astro islands only when needed
7. **Acessibilidade**: WCAG 2.1 AA mínimo
8. **Mobile-first responsive design**
9. **Site URL**: https://kodo-lang.dev

## Entregáveis

1. **Mockups/wireframes** de todas as seções da landing page
2. **Mockups/wireframes** da docs landing e de uma página de doc típica
3. **Componentes Astro** para as novas seções
4. **CSS/Tailwind** atualizado para o novo design
5. **Sidebar config** atualizada para a nova organização de docs
6. **Error Index page** redesenhada
7. **Stdlib Reference page** redesenhada
