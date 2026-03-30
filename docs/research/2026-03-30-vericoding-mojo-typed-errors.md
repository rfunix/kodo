# Research: Vericoding, Mojo 1.0 Typed Errors, and the AI-First Compiler Race

**Date**: 2026-03-30
**Agent**: Kōdo Architect (RESEARCH mode)
**Topics**: Vericoding benchmark, Mojo 1.0 H1 2026, Cranelift/MLIR state, AI agentic programming

---

## 1. Vericoding — O Novo Paradigma de Geração de Código Verificado

### O que é

"Vericoding" (cunhado em setembro de 2025, apresentado no POPL 2026) é o paradigma de usar LLMs para gerar código **formalmente verificado** — em contraste com "vibecoding" (código funcional mas sem garantias formais). O paper de referência criou o maior benchmark de verificação formal até hoje:

- 12.504 especificações formais
- 3.029 em Dafny, 2.334 em Verus/Rust, 7.141 em Lean

**Taxas de sucesso com LLMs off-the-shelf**:
- Dafny: 82% (subiu de 68% para 96% no último ano com LLMs melhores)
- Verus/Rust: 44%
- Lean: 27%

### Relevância para Kōdo: CRÍTICA

O Kōdo está **diretamente posicionado** no mercado de vericoding:
- Contratos `requires`/`ensures` verificados por Z3 estático
- Anotações `@confidence` + `@authored_by` para rastreabilidade
- JSON errors com `fix_patch` para loop error→fix automatizado

**Oportunidade imediata**: Submeter o Kōdo ao benchmark de vericoding do POPL 2026. A taxa do Dafny (82%) é alta porque Dafny foi _desenhado_ para verificação — o Kōdo também foi. Precisamos de dados comparativos.

**Ação recomendada**: Criar conjunto de tarefas Kōdo equivalentes ao AlgoVeri benchmark (2602.09464) — algoritmos clássicos com pré/pós-condições formais verificadas por Z3.

**Referências acadêmicas aplicáveis**:
- [SF] Software Foundations — Hoare logic como fundação dos contratos
- [CC] Ch.10-12 — SMT/Z3 para verificação automatizada
- POPL 2026 paper: "A benchmark for vericoding: formally verified program synthesis"
- ArXiv 2509.22908, 2507.13290

---

## 2. Mojo 1.0 H1 2026 — Ameaça Competitiva Direta

### Estado atual (Modular 26.1, janeiro 2026)

Mojo avança aceleradamente para o lançamento 1.0 previsto para H1 2026. Features críticas já implementadas:

#### Tipos Lineares (Explicitly-Destroyed Types)
Mojo agora tem suporte de primeira classe a tipos com destruição explícita — o que em jargão de PL são "linear types". Permite garantias em tempo de compilação de que certos valores não podem ser esquecidos (sem GC, sem overhead).

#### Typed Errors
```mojo
fn foo() raises CustomError -> Int:
    ...
```
Funções podem levantar tipos específicos em vez do genérico `Error`. Os erros tipados compilam para _alternate return value_ (sem stack unwinding) — adequados para GPU e embedded.

#### Melhoria de mensagens de erro
O maior pain point do Mojo era mensagens confusas de inferência de parâmetros. Agora:
- Diff de dois tipos similares mostrando qual sub-parâmetro diverge
- Mensagem clara quando tipos não batem (em vez de "não consegui inferir parâmetro")

### Análise competitiva

| Feature | Mojo 1.0 | Kōdo v1.11 |
|---------|----------|------------|
| Linear types | Sim (explicitly-destroyed) | Sim (own/ref/mut) |
| Typed errors | Sim (função raises CustomError) | Parcial (`Result<T, E>` mas E sempre String) |
| Contratos formais | Nao | Sim (Z3 estático) |
| Agent-first design | Parcial (AI/ML focado) | Sim (proposito central) |
| JSON errors / fix patches | Nao | Sim |
| @confidence / @authored_by | Nao | Sim |
| Alvo primario | AI/ML (GPU, tensor ops) | AI agents (correctness, auditability) |

**Diferenciação do Kōdo**: Mojo compete em performance de ML/GPU. Kōdo compete em **correctness e auditabilidade para agentes de software**. Os mercados são distintos mas há sobreposição no espaço "AI-first language".

**Risco**: Se Mojo 1.0 adicionar contratos formais (Z3 ou similar), o diferencial central do Kōdo fica ameaçado. Monitorar roadmap do Mojo.

**Ação recomendada**: Resolver a limitação de `Result<T, E>` (onde E é sempre String). Custom error enums end-to-end é bloqueador competitivo importante.

---

## 3. Cranelift e MLIR — Estado do Backend

### Cranelift (2026)
- Usado como backend alternativo do rustc (`rustc_codegen_cranelift`) — validação de maturidade
- Register allocator novo + ISLE DSL para instruction selection (2022-2023, agora estável)
- E-graphs para otimizações mid-end habilitados por padrão desde 2023
- Código ~2% mais lento que V8 TurboFan, ~14% mais lento que LLVM — mas compilação muito mais rápida
- Trade-off adequado para o Kōdo: feedback loop rápido > código hyper-otimizado

### Pliron (MLIR-inspired em Rust safe)
- Framework de IR extensível inspirado no MLIR, escrito em Rust safe
- Relevante para futuro redesign do `kodo_mir` se quisermos dialetos de IR modulares
- Ainda experimental, não pronto para produção

### Relevância para Kōdo
- O backend Cranelift do Kōdo está na direção certa — mesma escolha do rustc como alternativa
- MLIR/Pliron: interesse acadêmico para v3.0+, não acionável agora
- ISLE DSL: considerar para futuro `kodo_codegen` se instruction selection crescer em complexidade

---

## 4. AI Agentic Programming — Tendências Gerais

### Paradigma emergente
AI agentic programming = LLMs que planejam, executam e interagem com compiladores/debuggers para completar tarefas complexas iterativamente. O Kōdo foi **desenhado exatamente para esse paradigma**.

### Desafios que o Kōdo já endereça
- **Confiabilidade**: LLMs ainda não são adequados para aplicações críticas sem validação — o Kōdo resolve com contratos Z3 + `@confidence`
- **Feedback loop**: agentes precisam de erros machine-parseable — o Kōdo resolve com JSON errors + fix patches
- **Auditabilidade**: código gerado por agentes precisa ser rastreável — o Kōdo resolve com `@authored_by` + certificados

### Gap ainda não endereçado
O paper "AI Agentic Programming Survey" (ArXiv 2508.11126) aponta que agentes ainda têm dificuldade com:
- Raciocínio sobre concorrência e estado compartilhado
- Invariantes complexos entre módulos

**Oportunidade**: O sistema de `intent` blocks + contratos do Kōdo é exatamente a resposta — mas precisamos de documentação e benchmarks mostrando isso.

---

## 5. Sumário de Ações

| Prioridade | Ação | Impacto |
|-----------|------|---------|
| ALTA | Resolver `Result<T, E>` com custom error enums end-to-end | Paridade com Mojo typed errors |
| ALTA | Criar kodo-vericoding-bench (AlgoVeri equivalente) | Posicionar Kōdo no mercado de vericoding |
| MEDIA | Documentar diferenciação Kōdo vs Mojo explicitamente | Clareza de posicionamento |
| MEDIA | Monitorar roadmap Mojo 1.0 para additions de contratos formais | Alertas competitivos |
| BAIXA | Avaliar Pliron para redesign futuro do kodo_mir | v3.0+ planejamento |

---

## Referencias

- [Vericoding Benchmark POPL 2026](https://popl26.sigplan.org/details/dafny-2026-papers/13/A-benchmark-for-vericoding-formally-verified-program-synthesis)
- [ArXiv 2509.22908 — A Benchmark for Vericoding](https://arxiv.org/pdf/2509.22908)
- [Martin Kleppmann — AI will make formal verification go mainstream](https://martin.kleppmann.com/2025/12/08/ai-formal-verification.html)
- [Mojo Roadmap — Modular Docs](https://docs.modular.com/mojo/roadmap/)
- [Mojo Changelog](https://docs.modular.com/mojo/changelog/)
- [Modular 26.1 release](https://www.modular.com/blog/modular-26-1-a-big-step-towards-more-programmable-and-portable-ai-infrastructure)
- [Cranelift Dev](https://cranelift.dev/)
- [rustc_codegen_cranelift](https://github.com/rust-lang/rustc_codegen_cranelift)
- [AI Agentic Programming Survey — ArXiv 2508.11126](https://arxiv.org/html/2508.11126v1)
- [AlgoVeri Benchmark — ArXiv 2602.09464](https://arxiv.org/html/2602.09464)
