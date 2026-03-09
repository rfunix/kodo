# Academic References — Kōdo Compiler

This document maps academic literature to specific compiler phases and design decisions in Kōdo. AI agents working on the codebase should consult the relevant references when making design choices.

## Bibliography

| Abbr | Title | Author(s) | Scope |
|------|-------|-----------|-------|
| **[CI]** | *Crafting Interpreters* | Robert Nystrom | Lexer, parser, AST design, tree-walk interpretation — craftinginterpreters.com |
| **[EC]** | *Engineering a Compiler* | Keith Cooper & Linda Torczon | Full compiler pipeline: scanning, parsing, IR, optimization, code generation |
| **[TAPL]** | *Types and Programming Languages* | Benjamin C. Pierce | Type theory foundations, System F, subtyping, recursive types |
| **[ATAPL]** | *Advanced Topics in Types and PL* | Benjamin C. Pierce (ed.) | Dependent types, linear types, effect systems, module systems |
| **[SF]** | *Software Foundations* | Benjamin C. Pierce et al. | Formal verification, Hoare logic, proof assistants — softwarefoundations.cis.upenn.edu |
| **[CC]** | *The Calculus of Computation* | Aaron R. Bradley & Zohar Manna | Decision procedures, SMT solving, program verification |
| **[Tiger]** | *Modern Compiler Implementation in ML* | Andrew W. Appel | IR design, instruction selection, register allocation, garbage collection |
| **[PLP]** | *Programming Language Pragmatics* | Michael L. Scott | Language design trade-offs, scanning, parsing, type systems, concurrency, runtime |

## Reading Guide by Compiler Phase

### 1. Lexer (`kodo_lexer`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[CI]** | Ch. 4 | Scanner design, token representation, error recovery |
| **[EC]** | Ch. 2 | DFA-based scanning, maximal munch, table-driven vs hand-coded |
| **[PLP]** | Ch. 2 | Regular expressions, finite automata, scanner generators |

**Design decisions informed by:**
- **[CI]** Ch. 4: Hand-coded scanner for better error messages and simpler maintenance (we use `logos` for speed but follow the same token design philosophy).
- **[EC]** Ch. 2: Maximal munch rule for operators like `->`, `==`, `!=`.
- **[PLP]** Ch. 2: Classification of tokens into keywords, identifiers, literals, operators, and delimiters.

### 2. Parser (`kodo_parser`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[CI]** | Ch. 6–8 | Recursive descent, Pratt parsing, expression precedence |
| **[EC]** | Ch. 3 | LL(1) parsing, FIRST/FOLLOW sets, parse tables |
| **[PLP]** | Ch. 2.3 | Top-down parsing, predictive parsing, LL grammars |

**Design decisions informed by:**
- **[CI]** Ch. 6: Hand-written recursive descent for maximum control over error recovery and diagnostics.
- **[EC]** Ch. 3: LL(1) grammar design — every production is decidable by one token of lookahead.
- **[CI]** Ch. 8: Operator precedence via precedence climbing (Pratt-style) for expression parsing.

### 3. AST (`kodo_ast`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[CI]** | Ch. 5 | AST node design, visitor pattern, expression types |
| **[EC]** | Ch. 4–5 | IR taxonomy, AST vs CST, symbol tables |

**Design decisions informed by:**
- **[CI]** Ch. 5: Every node carries a `Span` for error reporting; typed enum variants for expressions/statements.
- **[EC]** Ch. 5: AST chosen over CST — we discard syntactic sugar at parse time for simpler downstream phases.

### 4. Type System (`kodo_types`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[TAPL]** | Ch. 1–11, 22–26 | Type safety, progress/preservation, System F, bounded quantification |
| **[ATAPL]** | Ch. 1 | Substructural type systems (linear, affine) |
| **[PLP]** | Ch. 7–8 | Type checking, type equivalence, parametric polymorphism |

**Design decisions informed by:**
- **[TAPL]** Ch. 8: Safety = progress + preservation. Every well-typed Kōdo program either produces a value or diverges — no undefined behavior.
- **[TAPL]** Ch. 22–26: System F as the theoretical basis for Kōdo's generics (`List<T>`, `Map<K, V>`).
- **[ATAPL]** Ch. 1: Linear/affine types for ownership (`own`/`ref`/`mut`) — values are used exactly once unless explicitly borrowed.
- **[PLP]** Ch. 7: No implicit type conversions — structural equivalence for generics, nominal for user types.

### 5. Contract Verification (`kodo_contracts`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[SF]** | Vol. 1–2 | Hoare logic, program correctness proofs, inductive propositions |
| **[CC]** | Ch. 1–6, 10–12 | Propositional/first-order logic, decision procedures, SMT solving |

**Design decisions informed by:**
- **[SF]** Vol. 1 (Logical Foundations): `requires`/`ensures` as Hoare triples — `{P} code {Q}` maps directly to function contracts.
- **[SF]** Vol. 2 (Programming Language Foundations): Operational semantics for reasoning about contract correctness.
- **[CC]** Ch. 10–12: Z3 SMT solver integration for automatic contract verification where decidable.
- **[CC]** Ch. 1–6: Propositional and first-order logic as the language of contract expressions.

### 6. Intent Resolver (`kodo_resolver`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[PLP]** | Ch. 10, 14–15 | Metaprogramming, compile-time code generation, macro systems |

**Design decisions informed by:**
- **[PLP]** Ch. 14–15: Intent resolution as a form of compile-time metaprogramming — agents declare goals, the compiler generates verified implementations.
- **[PLP]** Ch. 10: Module system design for organizing resolver strategies.
- Note: The intent system is a novel construct in Kōdo with no direct precedent in the literature. It draws loosely on metaprogramming and code generation concepts.

### 7. Mid-Level IR (`kodo_mir`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[Tiger]** | Ch. 7–8 | IR trees, canonical form, basic blocks, traces |
| **[EC]** | Ch. 5, 8–10 | IR design, data-flow analysis, SSA form, optimization |

**Design decisions informed by:**
- **[Tiger]** Ch. 7: Tree-based IR lowered to canonical form (no nested calls in expressions).
- **[Tiger]** Ch. 8: Basic blocks with single-entry/single-exit for clean CFG construction.
- **[EC]** Ch. 8–9: SSA form planned for optimization passes (currently direct assignment).
- **[EC]** Ch. 10: Data-flow analysis framework for liveness, reaching definitions, and borrow checking.

### 8. Code Generation (`kodo_codegen`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[Tiger]** | Ch. 9–11 | Instruction selection (tiling), register allocation (graph coloring) |
| **[EC]** | Ch. 11–13 | Instruction selection, scheduling, register allocation |

**Design decisions informed by:**
- **[Tiger]** Ch. 9: Tree-pattern matching for instruction selection (delegated to Cranelift).
- **[Tiger]** Ch. 11: Register allocation via graph coloring (handled by Cranelift's regalloc2).
- **[EC]** Ch. 11: Cranelift chosen over hand-coded instruction selection for faster compilation.
- **[EC]** Ch. 13: Register allocation and spilling managed by Cranelift's backend.

### 9. Standard Library (`kodo_std`)

| Reference | Chapters | Concepts |
|-----------|----------|----------|
| **[PLP]** | Ch. 6, 9, 13 | Control flow, subroutines, concurrency models |

**Design decisions informed by:**
- **[PLP]** Ch. 13: Structured concurrency model — no raw threads, only scoped tasks with ownership.
- **[PLP]** Ch. 6: Iterator protocol and control abstractions for collections.
- **[PLP]** Ch. 9: Subroutine calling conventions and parameter passing modes (`own`/`ref`/`mut`).
