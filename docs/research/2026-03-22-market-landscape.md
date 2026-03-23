# Pesquisa de Mercado — 2026-03-22

## Contexto

Pesquisa de mercado sobre linguagens compiladas, tendências em AI-assisted programming,
e inovações em type systems/contracts. Objetivo: identificar oportunidades e ameaças
para o posicionamento do Kōdo como linguagem para AI agents.

---

## 1. Panorama de Linguagens Compiladas (2025-2026)

### Rust
- Subiu de #19 para #14 no TIOBE. Consolidou-se como escolha padrão para infraestrutura segura.
- **Rust 2026 goals**: ~60 propostas em andamento. Destaques:
  - **Next-gen trait solver**: refatoração que desbloqueia implied bounds, negative impls, e corrige unsoundnesses.
  - **Polonius**: novo borrow checker que habilita "lending iterators" e padrões de borrowing mais expressivos. Meta: estabilização em 2026.
  - **In-place initialization**: criação de structs vinculados a locais em memória, desbloqueando `dyn Trait` com `async fn` e `-> impl Trait`.
- **Sem contracts nativos**: Rust ainda não tem RFC aceita para contracts (`requires`/`ensures`). Isso mantém o diferencial do Kōdo.

### Mojo
- **Primeira linguagem construída inteiramente sobre MLIR**. Permite compilação para GPUs, TPUs, ASICs.
- Compilador será open-source em 2026. Versão production-ready esperada para Q1 2026.
- Suporte a GPUs portáveis (NVIDIA + AMD) desde junho 2025.
- **Posicionamento**: Python-like com performance de C. Foco em AI/ML, não em agentes autônomos.
- **Relevância para Kōdo**: Mojo valida que linguagens novas podem ganhar tração se resolverem um problema claro. O target dele (AI/ML hardware) é ortogonal ao do Kōdo (AI agents escrevendo código).

### Zig
- Saltou de #149 para #61 no TIOBE. Competidor direto de C.
- Simplicidade radical: controle low-level com safety features opcionais (bounds checking, null checks, error unions).
- **Relevância para Kōdo**: Zig mostra que simplicidade vende. O Kōdo deve manter a sintaxe limpa e LL(1).

### Carbon
- Linguagem experimental do Google para suceder C++. Foco em interoperabilidade com C++ existente.
- Ainda em estágio experimental. Sem timeline de produção.
- **Relevância para Kōdo**: baixa. Carbon compete no espaço legacy C++.

### Gleam
- Roda na Erlang VM. Foco em concorrência e fault-tolerance.
- **Relevância para Kōdo**: O modelo de concorrência do Gleam (actor-based via BEAM) é maduro. Kōdo pode estudar o modelo para evoluir structured concurrency no futuro.

### Vale
- Abordagem inovadora: **generational references** + **region borrowing**.
- Cada objeto tem um "current generation" integer incrementado no free. Ponteiros carregam "remembered generation". Na desreferência, assert que os números batem.
- **Region borrowing**: o compilador sabe que durante um escopo, uma região de dados não será modificada, eliminando overhead de generation checks.
- Protótipo mostrou **zero overhead observável** quando usando linear style + regions.
- **Relevância para Kōdo**: ALTA. O modelo de Vale para memory safety sem borrow checker complexo é uma alternativa interessante ao modelo own/ref/mut do Kōdo. Vale que generational references permitem padrões que borrow checking proíbe (observers, back-references, graphs). Monitorar evolução.

### Roc
- Linguagem funcional pura com foco em performance e binários pequenos.
- **Relevância para Kōdo**: baixa diretamente, mas o foco de Roc em "platform hosts" (separar IO do código puro) é conceito interessante para sandboxing de agentes.

---

## 2. Tendências em AI-Assisted Programming

### Relatório Anthropic "2026 Agentic Coding Trends"

Oito tendências identificadas:

1. **Mudança de papel dos engenheiros**: de escrever código para orquestrar agentes. Foco em arquitetura, design e decisões estratégicas.
2. **De single-agent para multi-agent**: organizações deployam agentes especializados trabalhando em paralelo com context windows separados.
3. **Colaboração human-AI**: 60% do trabalho integra AI, com supervisão ativa em 80-100% das tarefas delegadas.
4. **Multi-agent coordination**: raciocínio paralelo em context windows separados é prática padrão.
5. **Scaling de agentic coding além de engenharia**: domain experts de outros departamentos podem usar.
6. **AI-automated review**: sistemas de review automatizado são essenciais para gerenciar output de agentes.
7. **Cross-functional adoption**: adoção multiplicativa de valor.
8. **Quality assurance em escala**: manter qualidade com throughput acelerado.

### Modelos Especializados por Linguagem
- Em 2026, surgiram modelos narrow-focused treinados exclusivamente nas regras de segurança e memória de ecossistemas específicos (Rust, Swift).
- **Oportunidade para Kōdo**: um modelo fine-tuned para Kōdo que entenda contracts, ownership linear, e intent blocks seria um diferencial competitivo enorme.

### MCP como Padrão
- MCP (Model Context Protocol) entrou na Linux Foundation e se tornou padrão para tool/data access em sistemas agênticos.
- **Kōdo já tem MCP server** — isso é um diferencial. Manter e expandir.

### Stack de Mercado
- GPT-5.2 lidera em lógica, Claude 4.5 em qualidade de engenharia, Gemini 3 em large context.
- Devstral (Mistral) foca em code-agent model.
- DeepSeek-V3.2 melhor open-source para reasoning e agentic workloads.

### Nenhuma Linguagem Concorrente para AI Agents
- **Achado crítico**: Não existe outra linguagem projetada especificamente para AI agents. O espaço é dominado por **ferramentas** (Cursor, Claude Code, Copilot, Devin) que trabalham com linguagens existentes.
- **Kōdo ocupa um nicho vazio**. Isso é simultaneamente uma oportunidade (first-mover) e um risco (o mercado pode não ver necessidade de uma linguagem nova).

---

## 3. Inovações em Type Systems e Contracts

### Contracts: Estado da Arte
- **Racket**: implementação nativa de contracts com ênfase em "blame assignment" — quando um contrato é violado, o sistema identifica qual parte do código é culpada com explicação precisa.
- **UC Berkeley (2025)**: "Constraint-behavior contracts" para componentes físicos usando equações implícitas. Foco em automação de verificação.
- **Integração type system + contracts**: tendência de tratar contracts como parte do sistema de tipos, não como anotações externas.

### Oportunidades para Kōdo
1. **Blame assignment aprimorado**: implementar blame tracking estilo Racket nos contracts do Kōdo. Quando um `requires` falha, identificar automaticamente qual caller violou a precondição e gerar fix patch.
2. **Contracts como tipos**: explorar a possibilidade de refinement types (`x: Int where x > 0`) que unificam constraints com o sistema de tipos.
3. **Contract inference**: inferir contracts automaticamente a partir do corpo da função (e.g., se a função faz `x / y`, inferir `requires { y != 0 }`).

---

## 4. Avaliação de Impacto para o Kōdo

### Ameaças
| Ameaça | Severidade | Mitigação |
|--------|------------|-----------|
| Rust adotar contracts nativos | Alta | Kōdo já tem contracts + Z3; manter liderança em DX |
| Mojo capturar mindshare "nova linguagem" | Média | Posicionamento diferente (AI agents vs AI/ML hardware) |
| Ferramentas como Cursor/Devin tornam linguagem irrelevante | Média | Kōdo oferece garantias que ferramentas sobre linguagens existentes não podem |
| Modelos narrow-focused para Rust | Baixa | Criar modelo fine-tuned para Kōdo |

### Oportunidades
| Oportunidade | Prioridade | Ação |
|--------------|------------|------|
| Nicho vazio de "linguagem para AI agents" | Crítica | Marketing e developer relations focados |
| Multi-agent coordination (Anthropic report) | Alta | Expandir MCP server para suportar multi-agent workflows |
| Blame assignment em contracts | Alta | Implementar blame tracking estilo Racket |
| Contract inference automática | Média | Pesquisar viabilidade com Z3 |
| Modelo fine-tuned para Kōdo | Média | Coletar dataset de código .ko para fine-tuning |
| Region borrowing (Vale) | Baixa | Monitorar; considerar para v2.0 |

---

## Fontes

- [Semaphore - Top 8 Emerging Programming Languages 2025](https://semaphore.io/blog/programming-languages-2025)
- [CodeCrafters - 7 New Programming Languages](https://codecrafters.io/blog/new-programming-languages)
- [Rust in 2026 - Medium](https://medium.com/@blogs-world/rust-in-2026-what-actually-changed-whats-trending-and-what-to-build-next-d70e38a4ad97)
- [Rust Project Goals 2026](https://rust-lang.github.io/rust-project-goals/)
- [Mojo Roadmap - Modular](https://docs.modular.com/mojo/roadmap/)
- [Mojo MLIR-Based HPC - arXiv](https://arxiv.org/html/2509.21039v1)
- [Vale - Generational References](https://verdagon.dev/blog/generational-references)
- [Vale - First Regions Prototype](https://verdagon.dev/blog/first-regions-prototype)
- [Anthropic 2026 Agentic Coding Trends Report](https://resources.anthropic.com/2026-agentic-coding-trends-report)
- [Anthropic Report Summary - tessl.io](https://tessl.io/blog/8-trends-shaping-software-engineering-in-2026-according-to-anthropics-agentic-coding-report/)
- [MIT Technology Review - AI Coding](https://www.technologyreview.com/2025/12/15/1128352/rise-of-ai-coding-developers-2026/)
- [Addy Osmani - LLM Coding Workflow 2026](https://addyosmani.com/blog/ai-coding-workflow/)
- [JetBrains - Best AI Models for Coding](https://blog.jetbrains.com/ai/2026/02/the-best-ai-models-for-coding-accuracy-integration-and-developer-fit/)
- [UC Berkeley - Contract-Based Design Automation](https://www2.eecs.berkeley.edu/Pubs/TechRpts/2025/EECS-2025-84.pdf)
