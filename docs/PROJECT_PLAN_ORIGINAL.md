> **Public project note:** This is the initial architecture and roadmap document, preserved for historical context. It describes the intended end state, not a claim that every item is implemented today. See `PROJECT_STATUS.md` and `CONFORMANCE.md` for the current implementation state and verified gaps.
>
> Internal machine hostnames, provider credentials, local deployment archives, and personal account details are intentionally not part of this public copy.

---
# HARNESS-RS — Plano completo

*Nome provisório. Harness de agentes de código em Rust, com modelo de plugins inspirado no Cordis, \50 ferramentas nomeadas (48 prontas e 2 adiadas), skills, memória de longo prazo e teto duro de 1,5 GB de RAM.*

Preparado para Samuel em 09/10/2026.

## 1. Resumo executivo

O objetivo é construir um harness próprio que compita de verdade com o Codex CLI e o Claude Code, mas que rode numa máquina pequena. O núcleo é em Rust, a extensibilidade segue o modelo do Cordis (contexto em árvore, ciclo de vida, serviços injetáveis, eventos), e tudo respeita um limite de 1,5 GB de RAM para o conjunto inteiro.

**Decisões centrais do plano:**

1. **Um daemon Rust único e clientes finos.** O núcleo (loop do agente, ferramentas, memória, permissões) roda em um processo. O **HUD atual é o cliente nº 1 desde a Fase 1**; a TUI e as IDEs vêm depois e falam com o mesmo daemon por HTTP, SSE e WebSocket.
2. **Cordis como modelo, não como dependência do núcleo.** O Cordis é um framework TypeScript e não roda dentro de um binário Rust. Reimplementamos em Rust os quatro conceitos dele (Context, Fibers, Services, Events) e deixamos um host Node opcional, ligado só sob demanda, para plugins escritos em TypeScript com o Cordis de verdade.
3. **As 46 ferramentas do Claude Code, com os mesmos nomes.** O conjunto-alvo é a lista oficial (Read, Write, Edit, Bash, Agent, Skill, LSP, Cron\* e as demais), com os mesmos parâmetros e comportamento, para que permissões, hooks e skills escritos para o Claude Code funcionem sem mudança. Só 15 ficam carregadas por padrão; as outras 31 entram por `ToolSearch`. Mais 4 extensões nossas (memória e `ApplyPatch`) ficam fora da contagem.
4. **O HUD desde o primeiro dia.** O gateway fala o contrato que o HUD atual já usa (SSE em `/api/dsh-chat`, aprovações, perguntas, histórico, upload). O `thomas-hud.html` roda sem alteração já na Fase 1 e é a interface principal.
5. **Memória em um único SQLite** (FTS5, grafo de relações e vetores opcionais), com falhas sempre visíveis. Nada de falhar em silêncio.
6. **Compatibilidade desde o dia 1:** `AGENTS.md`, `SKILL.md`, MCP e o vocabulário de hooks que Claude Code e Codex já usam. Quem tem skills e hooks hoje não precisa reescrever.
7. **1,5 GB como teto duro**, imposto por cgroup, mais um controle de admissão para os componentes pesados (navegador, servidor LSP, host Node).
8. **"Competir" significa vencer onde o harness decide:** confiabilidade de edição, eficiência de tokens, uso de recursos, memória e extensibilidade. A capacidade bruta do modelo continua sendo do modelo.

**Estimativa:** MVP utilizável em cerca de 8 semanas e versão 1.0 em cerca de 6 meses, para uma pessoa dedicando 15 a 20 horas por semana. Os números são aproximados e devem ser revistos ao fim da Fase 0.

## 2. Premissas (confirme ou corrija)

1. **Ferramentas: o conjunto é o dos 46 do Claude Code**, conforme a documentação oficial consultada em 09/10/2026. O estudo de código-fonte contou 43 numa versão anterior: o número varia por versão e por plataforma, então a Fase 0 refaz a conferência. Isso substitui a ideia anterior de 60 ferramentas próprias.
2. **"1,5 de RAM" foi lido como 1,5 GB para o harness inteiro:** daemon, plugins, memória e processos filhos. O modelo é remoto (API). Um modelo local rodando ao lado fica fora do teto e precisa de orçamento próprio.
3. **Alvo: Linux x86\_64.** A máquina atual é Debian 13 com Atom N2600 (2 núcleos, sem AVX) e cerca de 1,95 GB físicos. As consequências estão nas seções 13 e 15.
4. **Rust estável**, edição 2024, workspace com vários crates pequenos.
5. **Licença sugerida: Apache-2.0.** O Codex CLI é Apache-2.0 e escrito em Rust, então dá para estudar e reaproveitar peças dele (sandbox, `apply_patch`, política de execução) com a devida atribuição. O Claude Code é proprietário: a compatibilidade com ele deve se basear só em documentação pública.
6. **O HUD de arquivo único (`thomas-hud.html`) faz parte do escopo desde o início.** Ele será enviado ao chat que vai construir o harness, junto com este documento, e serve como especificação viva da interface (seção 5.5).
7. **O stack atual continua de pé** (DSH em TypeScript) até o novo estar pronto. O HUD aponta para um ou para outro por configuração, e o Thomas mobile agent fica como está.

## 3. Metas mensuráveis e o que significa competir

### 3.1 Metas (SLOs)

- **RAM:** pico de até 1,5 GB (imposto por cgroup). Uso típico em sessão normal de até 600 MB. Daemon ocioso em até 80 MB de RSS.
- **Partida:** daemon frio em até 300 ms. Overhead do harness no primeiro token de até 50 ms além da latência do provedor.
- **Ferramentas leves:** `read_file`, `glob` e `grep` em até 20 ms (p95) em repositório de 10 mil arquivos.
- **Confiabilidade de edição:** `edit_file` e `apply_patch` com 98% ou mais de sucesso na primeira tentativa no benchmark interno, e zero arquivos corrompidos (escrita atômica).
- **Eficiência de tokens:** pelo menos 30% menos tokens por tarefa do que uma versão sem ferramentas adiadas e sem compactação (medir, não supor).
- **Segurança:** zero escapes de sandbox na suíte de testes; toda escrita e toda execução auditadas.
- **Estabilidade:** sessões de 8 horas sem crescimento de RSS acima de 10%.
- **Compatibilidade:** executar skills e hooks existentes sem alteração em pelo menos 90% dos casos de teste.

### 3.2 O que "competir com Codex e Claude Code" quer dizer

Um estudo de código-fonte de onze harnesses de produção (arXiv 2609.00006) mostra que a diferença entre eles está em subsistemas concretos: ferramentas, permissões, orquestração e contexto. Ele também mostra convergência: o Codex copiou o vocabulário de hooks do Claude Code, e o OpenHands importa o formato de plugins dele. A lição é que **compatibilidade é barata e vale muito**.

**Onde dá para ganhar:**

- **Footprint.** Rust com 1,5 GB de teto e daemon ocioso abaixo de 80 MB (medir contra os concorrentes na mesma máquina).
- **Memória de longo prazo de primeira classe**, não um recurso lateral.
- **Neutralidade de provedor:** qualquer modelo por API, trocável no meio da sessão.
- **Acesso remoto e mobile** (HUD pela Tailscale), que os concorrentes tratam como acessório.
- **Confiabilidade de edição e eficiência de contexto**, que são engenharia pura.

**Onde não dá para prometer:** capacidade bruta do modelo, sandboxes em nuvem e escala de equipe. O Codex tem integração vertical com a infraestrutura da OpenAI. O plano não tenta replicar isso.

## 4. Cordis + Rust: como combinar de verdade

### 4.1 O que o Cordis é

- Framework TypeScript de código aberto (MIT), nascido do bot Koishi, descrito como um meta-framework: serve para construir outros frameworks.
- Quatro blocos: **Context** (árvore de contextos; cada plugin recebe o seu `ctx`), **Fibers** (ciclo de vida de cada plugin), **Services** (injeção de dependência por nome, com `provide` e `inject`) e **Events** (barramento com ciclo de vida).
- Recarrega plugins a quente e desfaz tudo na ordem inversa (pilha LIFO de efeitos), o que evita vazamentos.
- É o substrato de plugins do DeepSeek Harness, ou seja, do stack TypeScript que você já roda.
- Existem reimplementações do mesmo modelo em Ruby e em Python, baseadas no artigo "A Programming Paradigm for Spatiotemporal Composability". Isso mostra que o modelo é portável: ele não depende de JavaScript.

### 4.2 O problema e as três opções

O Cordis roda em Node. Um binário Rust não importa pacotes npm. Opções:

- **Opção A. Tudo em Node com Cordis; Rust só para ferramentas.** É a mais simples, mas o loop e o gerenciamento de memória ficam em JavaScript, com RAM fixa maior e sem o ganho de Rust onde importa.
- **Opção B. Núcleo Rust com um sidecar Node permanente rodando o Cordis.** Funciona, mas paga RAM fixa (processo Node sempre ligado) e uma ponte de IPC em cada chamada de plugin.
- **Opção C. Núcleo Rust com `cordis-rs` (o modelo reimplementado) e um host Node opcional, ligado só quando um plugin TypeScript é ativado.** RAM mínima no caso comum, e quem quiser plugin em TS usa o Cordis real.

**Decisão: Opção C.** É a única que cumpre o teto de RAM sem abrir mão de poder usar o ecossistema Cordis.

### 4.3 O que é o `cordis-rs`

Mapeamento conceito por conceito:

- **Context → `Ctx`.** Árvore de contextos em `Arc`. Cada plugin recebe um `Ctx` filho. Descartar um filho descarta tudo que foi registrado nele.
- **Plugin → trait `Plugin`.** Declara nome, configuração, dependências (`inject`) e uma função `apply`.
- **Fiber → `Fiber`.** Máquina de estados do plugin: `Pending → Loading → Active → Disposing → Disposed`, com `Failed` e recuo exponencial (backoff).
- **Effects → pilha de descarte.** Tudo que um plugin registra (ferramenta, hook, serviço, tarefa de fundo) entra numa pilha e sai em ordem inversa, por `Drop` e cancelamento (`CancellationToken`).
- **Services → registro tipado.** `provide` e `inject` por chave. Um plugin só ativa quando todos os serviços que ele declara estão disponíveis, e volta a `Pending` se um deles sumir. Há isolamento: a mesma chave pode ter instâncias diferentes em escopos diferentes (útil para subagentes).
- **Events → barramento tipado** com quatro modos: **emit** (todos os ouvintes, em paralelo), **serial** (um por vez, em ordem), **bail** (o primeiro que responder encerra) e **waterfall** (o valor passa de ouvinte em ouvinte e é transformado). Os pontos de gancho (hooks) do harness são eventos desse barramento.
- **Hot reload.** Descartar o escopo do plugin e aplicá-lo de novo, sem reiniciar o daemon.
- **Configuração.** Esquema por plugin gerado com `serde` e `schemars`, validado antes do `apply`.

### 4.4 Contrato principal (esboço)

```rust
pub trait Plugin: Send + Sync + 'static {
    const NAME: &'static str;
    type Config: DeserializeOwned + JsonSchema + Default;

    /// Servicos que precisam existir antes do apply.
    fn inject() -> &'static [ServiceKey] { &[] }

    /// Registra ferramentas, hooks e servicos no ctx recebido.
    /// Tudo o que for registrado aqui e descartado em ordem inversa no dispose.
    async fn apply(ctx: Ctx, cfg: Self::Config) -> Result<()>;
}

// Uso dentro de um plugin:
ctx.provide::<MemoryStore>(store);
ctx.tool(ReadFile::spec(), ReadFile::run);
ctx.on(Hook::PreToolUse, Mode::Bail, |ev| async { /* politica */ });
ctx.effect(|| background_job());   // cancelado no dispose
```

### 4.5 Host Node para plugins TypeScript

- Fala **JSON-RPC por stdio** com o daemon. É o mesmo protocolo usado por qualquer plugin fora do processo (veja a seção 11).
- Um SDK `@harness/plugin` embrulha o Cordis real. O `ctx` do plugin TS é uma subárvore espelhada da árvore Rust, e serviços Rust (memória, ferramentas) chegam por proxy.
- **Ativação lazy:** o host só sobe quando um plugin TS é necessário e é desligado após 5 minutos ocioso.
- **Limite de RAM** do host: 150 MB (`memory.max` do cgroup filho). Se estourar, é reiniciado e o plugin recebe o evento de falha.

## 5. Arquitetura geral

```text
+------------------+   +------------------+   +------------------+
|  TUI (ratatui)   |   |  HUD web/mobile  |   |  IDE (ACP / MCP) |
+---------+--------+   +---------+--------+   +---------+--------+
          |  HTTP/WebSocket/SSE (gateway)                |
+---------v-----------------------v----------------------v--------+
|                     DAEMON  harness-rsd (Rust)                   |
|  hs-gateway  | hs-agent (loop) | hs-ctx (contexto) | hs-model     |
|  hs-tools    | hs-perm (politica) | hs-sandbox | hs-skills        |
|  hs-memory (SQLite) | hs-plugins (cordis-rs) | hs-obs (logs)       |
+---------+----------------+------------------+--------------------+
          |                |                  |
   plugins externos   host Node (opcional)   processos filhos
   (JSON-RPC stdio)   Cordis real, lazy      (bash, LSP, browser)
```

### 5.1 Crates do workspace

- `hs-core`: `Ctx`, `Fiber`, serviços, eventos, configuração, erros.
- `hs-agent`: máquina de estados do turno, streaming, cancelamento, subagentes.
- `hs-model`: abstração de provedores (Anthropic, OpenAI, Gemini, DeepSeek, OpenRouter, qualquer API compatível com OpenAI), SSE, análise de chamadas de ferramenta, controle de cache de prompt.
- `hs-ctx`: montagem do prompt, contagem de tokens, compactação.
- `hs-tools`: registro, esquemas e execução das 50 ferramentas do catálogo verificado.
- `hs-perm` e `hs-sandbox`: política de permissões e isolamento.
- `hs-skills`, `hs-memory`, `hs-plugins`.
- `hs-gateway`: API HTTP/WebSocket e sessões.
- `hs-cli`: TUI e comandos (`harness doctor`, `harness eval`, `harness skill ...`).
- `hs-eval`: benchmarks e testes de regressão de RAM, tokens e tempo.

Crates pequenos compilam em paralelo e permitem desligar recursos por `feature` (por exemplo, compilar sem navegador).

### 5.2 Modelo de threads

- Runtime `tokio` com **2 threads de trabalho** (a máquina tem 2 núcleos) e um pool de bloqueio pequeno (4) para I/O de disco e buscas.
- Canais sempre **limitados** (bounded), para que um consumidor lento aplique contrapressão e não encha a RAM.
- Subagentes são **tarefas assíncronas dentro do daemon**, não processos. Isso custa poucos MB cada, em vez de dezenas.

### 5.3 Dados em disco

- `~/.harness/config.toml`: configuração e provedores (sem chaves; chaves ficam no cofre do sistema ou em arquivo `0600` separado).
- `~/.harness/sessions/`: um arquivo JSONL por sessão (event sourcing) mais *snapshots* periódicos.
- `~/.harness/memory.db`: SQLite único da memória.
- `~/.harness/skills/` e `<projeto>/.harness/skills/`: skills. Também lê `.claude/skills` e `.agents/skills` por compatibilidade.
- `~/.harness/plugins/`: plugins instalados, com `plugins.lock` (hashes fixados).
- `~/.harness/logs/`: logs com rotação e filtro de segredos.

### 5.4 Gateway

- API local `HTTP + SSE/WebSocket` com autenticação por token de longa duração, vinculada a 127.0.0.1 por padrão. Acesso remoto só por túnel privado (Tailscale), nunca aberto à internet por padrão.
- **Dois contratos no mesmo gateway:** o contrato nativo (`/v1/...`: criar e retomar sessão, enviar mensagem, fluxo de eventos do turno, aprovar ou negar permissão, listar ferramentas, memória e plugins, métricas) e o **contrato do HUD** (seção 5.5), que é uma camada de compatibilidade em cima do nativo.
- O mesmo fluxo de eventos alimenta HUD, TUI e IDE. A camada do HUD vive num módulo embutido, `hud-compat`, para poder mudar ou sair sem tocar no núcleo.
- Suporte ao **ACP** (Agent Client Protocol) para IDEs e a **MCP** como cliente e como servidor.

### 5.5 O HUD como cliente nº 1, desde a Fase 1

O HUD atual (`thomas-hud.html`, arquivo único de cerca de 545 KB) será enviado ao chat que construir o harness, junto com este documento. Ele é a **especificação viva da interface**: o gateway deve fazer o HUD funcionar **sem alterar o arquivo** (a não ser o endereço base, se for preciso) já na Fase 1.

**Como o HUD conversa hoje** (levantado do servidor atual e do próprio HUD):

- **Entrega:** o servidor serve o HTML em `/` como bytes, sem reescrever nada. As chamadas são relativas (mesma origem).
- **Chat:** `POST /api/dsh-chat` devolve um fluxo **SSE**: blocos separados por linha em branco, cada um com linhas `data: {json}`. A parada é `POST /api/dsh-stop` com `session_id` e `run_id`.
- **`run_id` estável** durante todo o turno do usuário.
- **Eventos que o HUD entende:** `status`, `text_delta`, `text`, `thinking_delta`, `tool_call`, `tool_result`, `tool_observation`, `question_request`, `approval_request`, `interaction_resolved`, `run_registered`, `provider_retry`, `context_injection`, `heartbeat`, `hud_boot`, `deliverable` e os terminais `final`, `turn_end`, `cancelled` e `error`.
- **Aprovações e perguntas:** respostas por `POST /api/dsh-respond`; pendentes em `/api/dsh-pending`.

**Rotas que o HUD usa, por grupo:**

- **Chat e execução:** `/api/dsh-chat`, `/api/dsh-stop`, `/api/dsh-run`, `/api/dsh-respond`, `/api/dsh-pending`, `/api/dsh-warm`, `/api/warm-status`, `/api/chat-mesh`.
- **Estado e configuração:** `/api/health`, `/api/settings`, `/api/providers`, `/api/models`, `/api/models-batch`, `/api/local-models`, `/api/apis`, `/api/discover-key`, `/api/harness-effort`.
- **Histórico e arquivos:** `/api/history`, `/api/history-clear`, `/api/upload`, `/api/uploads`, `/api/upload-delete`, `/api/system-storage` e `/api/workspace/` (`list`, `projects`, `photos`, `terminal/tree`, `terminal/file`, `terminal/raw`).
- **Extensões e aprendizado:** `/api/harness-plugins`, `/api/harness-skills`, `/api/plugin-install`, `/api/learnings`, `/api/learning-candidate`, `/api/learning-candidates`, `/api/learning-reset`.
- **Agendamento e avisos:** `/api/events`, `/api/events/trigger/`, `/api/notification-capabilities`.
- **Navegador e celular:** `/api/browser/` (`status`, `navigate`, `frame`, `input`) e `/api/mobile/` (`devices`, `command`, `command-all`, `frame`, `pairing`, `apk`).
- **Voz:** `/voice` e `/voice/...`.

**Mapa de eventos proposto (harness para HUD):**

- Streaming do modelo vira `text_delta` e `thinking_delta`; o texto completo vira `text`; o fim do turno vira `final` e `turn_end`.
- Chamada de ferramenta vira `tool_call`; o resultado vira `tool_result`; resumos de leitura e compactação viram `tool_observation`. **O HUD mostra o nome da ferramenta na trajetória**, então os nomes oficiais (`Read`, `Edit`, `Bash`...) aparecem como são.
- `AskUserQuestion` vira `question_request` (botões no HUD); um pedido de permissão vira `approval_request`; a resposta vira `interaction_resolved`.
- A memória injetada no prompt vira `context_injection`, mostrando ao usuário o que foi recuperado.
- Retentativa de provedor vira `provider_retry`.
- `TaskCreate`, `TaskUpdate` e `TodoWrite` viram `status` com a lista de tarefas. `Artifact` e `SendUserFile` viram `deliverable`.
- `heartbeat` periódico, para conexões ociosas não caírem.

**Como o chat que construir deve proceder:**

1. **Extrair o contrato do próprio HUD antes de escrever o gateway.** As rotas e os eventos acima são um levantamento; a fonte de verdade é o arquivo. Por exemplo: `timeout 20 grep -oE 'hudJson\("/api/[a-z/_-]+' thomas-hud.html | sort -u`.
2. **Implementar o contrato no módulo `hud-compat`** (plugin embutido do `hs-gateway`) sobre o fluxo nativo de eventos.
3. **Rotas de recursos que ainda não existem** (voz, navegador, celular) respondem `{"ok": false, "reason": "not_implemented"}` com HTTP 200, para o HUD degradar sem erro até a fase em que elas chegam.
4. **Critério de aceitação da Fase 1:** abrir o HUD, conversar com streaming, ver a trajetória de ferramentas com nomes, aprovar e negar permissões, responder perguntas, parar um turno, ver o histórico, enviar um arquivo e trocar de modelo nas configurações.
5. **Não editar o `thomas-hud.html` até a Fase 3**, exceto o endereço base. Mudanças de interface vêm depois, com o contrato estável.

## 6. O loop do agente

### 6.1 Máquina de estados do turno

```text
Idle -> Assembling -> Streaming -> (ToolPlanning -> ToolRunning -> Observing) -> Streaming ... -> Done
                                      ^                                   |
                                      +-------------- repete ------------+
```

1. **Assembling:** o `hs-ctx` monta o prompt na ordem que preserva o cache (seção 7).
2. **Streaming:** resposta do modelo por SSE. O texto vai ao cliente conforme chega. Blocos de chamada de ferramenta são acumulados e validados contra o esquema assim que fecham.
3. **ToolPlanning:** o daemon decide o que roda em paralelo. Ferramentas somente de leitura rodam juntas. Escritas no mesmo caminho são serializadas.
4. **ToolRunning:** cada ferramenta recebe um `Ctx` filho com timeout, cancelamento e limite de saída. Permissões são checadas antes (seção 12).
5. **Observing:** resultados voltam ao modelo, já truncados ou transferidos para disco se forem grandes (seção 7.3).
6. **Done:** o modelo encerra sem pedir ferramentas, ou um limite foi atingido.

### 6.2 Condições de parada e limites

- Máximo de passos por turno (padrão 60), de tokens por turno e de custo por sessão, todos configuráveis.
- Detecção de laço: se a mesma chamada de ferramenta com os mesmos argumentos falhar 3 vezes seguidas, o loop injeta uma mensagem de sistema pedindo uma abordagem diferente. Se persistir, para e pede ajuda ao usuário.
- Cancelamento (`Ctrl+C` ou botão no HUD) interrompe o streaming, cancela as ferramentas em execução e fecha a sessão em estado consistente.

### 6.3 Robustez

- **Reconexão e retentativa** com recuo exponencial para erros 429 e 5xx. Respeita `Retry-After`.
- **Troca de provedor** no meio da sessão: o histórico é guardado em formato neutro e convertido por provedor.
- **Sessões retomáveis:** o log de eventos é a fonte da verdade. Retomar é reexecutar o log para reconstruir o estado. Fork de sessão é copiar o log até um ponto.
- **Checkpoints antes de escritas:** cada edição guarda um *snapshot* do arquivo (cópia com hash). O comando `/rewind` desfaz até um ponto.
- **Erros de ferramenta** voltam ao modelo como resultado normal, com mensagem acionável (o que falhou e o que tentar), nunca como exceção que derruba o turno.

### 6.4 Modos de operação

- **plan:** somente leitura. O agente explora e propõe um plano; só executa depois de aprovado.
- **ask:** pede aprovação para cada escrita ou execução.
- **auto-edit:** edições dentro do projeto liberadas; execução ainda pede aprovação.
- **auto:** tudo liberado dentro das regras da política e do sandbox.

### 6.5 Subagentes

- Ferramenta `Agent` cria um agente com **contexto próprio**, lista de ferramentas permitidas, limite de passos e orçamento de custo. O agente principal só recebe o resumo final.
- Servem para **explorar sem poluir o contexto principal**, para trabalho em paralelo e para revisão independente.
- Um subagente nunca recebe mais permissões do que o agente que o criou (interseção de políticas).
- Profundidade máxima de 2 e no máximo 6 subagentes simultâneos, para segurar RAM e custo.

## 7. Gestão de contexto

O contexto é o recurso mais caro do harness: custa tokens, dinheiro e latência. A regra é **enviar o mínimo útil, na ordem certa**.

### 7.1 Layout do prompt (ordem importa para o cache)

1. **Sistema base** (estático): papel, regras de uso de ferramentas, estilo. Alvo: até 3 mil tokens.
2. **Esquemas das ferramentas do núcleo** (estático, as 15 sempre carregadas). Alvo: até 3 mil tokens.
3. **Instruções do projeto:** `AGENTS.md` (e `CLAUDE.md` por compatibilidade), com limite de 4 mil tokens e regras por subdiretório carregadas só quando um arquivo daquela pasta é tocado.
4. **Índice de skills** (nome e descrição curta de cada uma). Alvo: até 1,5 mil tokens.
5. **Resumo de memória** relevante à tarefa. Alvo: até 2 mil tokens.
6. **Conversa**, com resultados de ferramentas.

O que muda pouco fica no começo, e o que muda a cada turno fica no fim. **O prefixo nunca é alterado no meio da sessão**, a não ser numa compactação, que custa uma perda de cache planejada. Provedores que cobram menos por prefixo em cache passam a economizar de forma significativa.

### 7.2 Contagem de tokens

- Usar o endpoint de contagem do provedor quando existir.
- Caso contrário, estimativa local (tokens ≈ caracteres ÷ 4, calibrada pela taxa real que o provedor devolve em cada resposta) e correção contínua com os números de uso reportados.
- Orçamento por seção (7.1) e reserva de saída (padrão 16 mil tokens) subtraídos da janela do modelo.

### 7.3 Compactação em camadas (da mais barata para a mais cara)

1. **Truncar na entrada.** Resultado de ferramenta acima do limite (padrão 30 mil caracteres) é gravado em disco; o modelo recebe um trecho inicial e um identificador, e lê o resto por fatias com `read_file`.
2. **Mascarar observações antigas.** Resultados de ferramentas com mais de N turnos (padrão 8) viram uma linha-resumo (`read_file src/a.rs: 340 linhas, omitido`), preservando a chamada e removendo o conteúdo.
3. **Deduplicar leituras.** O mesmo arquivo lido de novo sem mudança (mesmo hash) devolve uma referência, não o conteúdo.
4. **Checkpoint por resumo.** Ao chegar a 70% da janela, um modelo barato resume a conversa num formato fixo: objetivo, decisões tomadas, arquivos tocados (com o que mudou), pendências, restrições do usuário e erros já resolvidos. O resumo substitui o histórico antigo; os últimos turnos ficam intactos.
5. **Subagente de exploração.** Antes de ler muito código, o agente delega a busca a um subagente e recebe só as conclusões.

A 90% da janela, o harness recusa novas leituras grandes e força a compactação.

### 7.4 Persistência e retomada

- O resumo de compactação e o log completo ficam em disco. Nada se perde; o que o modelo vê é uma visão reduzida.
- Comando `/context` mostra, por seção, quantos tokens cada parte ocupa. Isso torna o consumo auditável.

## 8. Catálogo de ferramentas: 50 nomes na API

**Estado verificado em 10/10/2026:** o catálogo Rust contém 50 nomes únicos; 48 têm despacho implementado e 2 permanecem adiados. O plano original usava a lista de 46 ferramentas do Claude Code como referência de compatibilidade. Isso não deve ser confundido com paridade exata: o catálogo implementado inclui nomes específicos do Harness, e cada ferramenta precisa ser comparada individualmente com a referência pública antes de afirmar compatibilidade de parâmetros e resultados.

A fonte de verdade da implementação é `src/tool_catalog.rs`, exposta por `/api/harness/tools`. O prefixo inicial de 15 ferramentas é enviado ao modelo; as demais ferramentas prontas são descobertas sob demanda por `ToolSearch`. Ferramentas adiadas continuam visíveis no catálogo, mas não são oferecidas ao modelo como executáveis.

**Significado de “pronta”:** existe um caminho de despacho no runtime. Isso não garante que um serviço externo esteja instalado ou ligado. Por exemplo, MCP e LSP exigem servidores explicitamente configurados; voz, navegador e mobile dependem dos respectivos serviços upstream.

### 8.1 Regras de qualidade e compatibilidade

- Cada ferramenta tem nome estável, descrição, esquema JSON, grupo, estado de disponibilidade e política de permissão.
- O conjunto que se sobrepõe ao Claude Code deve ser comparado contra a documentação pública; diferenças de plataforma e extensões do Harness são documentadas, não disfarçadas como paridade.
- Saídas são limitadas e erros devem ser acionáveis. Limites de tamanho, timeout, cancelamento e permissões são aplicados no dispatcher sempre que a operação os suporta.
- Uma ferramenta só pode ser marcada `ready` quando existe despacho real e teste apropriado. Um endpoint HTTP que apenas encaminha a um upstream não prova que esse upstream está funcional.
- O teste de schemas em `tests/tool_schema_conformance.rs` valida os esquemas serializados. Testes unitários e E2E complementam a validação; a existência do esquema, isoladamente, não prova a execução da ferramenta.

### 8.2 As 15 ferramentas do prefixo inicial

1. **`Read`** — lê arquivos como linhas numeradas, com `offset` e `limit`; não lê diretórios.
2. **`Write`** — cria ou substitui arquivo atomicamente; exige leitura prévia para arquivos existentes.
3. **`Edit`** — substitui um trecho exato e único; exige leitura prévia, salvo regra explícita de substituição múltipla.
4. **`NotebookEdit`** — edita células de notebook por índice ou identificador.
5. **`Glob`** — encontra caminhos por padrão glob, com resultados limitados e ordenados.
6. **`Grep`** — pesquisa conteúdo por expressão regular, com limite de resultados e respeito ao `.gitignore` por padrão.
7. **`Bash`** — executa um comando no workspace com timeout, saída limitada e política de permissões.
8. **`Agent`** — inicia subagente com contexto próprio e permissões limitadas pela interseção das permissões do agente pai.
9. **`TaskCreate`** — cria tarefa na lista da sessão.
10. **`TaskGet`** — recupera detalhes de uma tarefa da sessão.
11. **`TaskList`** — lista tarefas da sessão e seus estados.
12. **`TaskUpdate`** — atualiza tarefa e dependências; os IDs e ciclos de dependência são validados.
13. **`TodoWrite`** — substitui a lista ordenada de passos da sessão.
14. **`Skill`** — carrega `SKILL.md` local e aplica suas instruções sem elevar permissões.
15. **`ToolSearch`** — pesquisa o catálogo completo e carrega ferramentas prontas relevantes para o turno.

### 8.3 As outras 35 ferramentas

As ferramentas abaixo não fazem parte do prefixo inicial. As prontas podem ser carregadas com `ToolSearch`; as adiadas são listadas para transparência, mas não executadas.

**Execução e tarefas**

- `Monitor` — consulta uma tarefa em segundo plano e lê saída limitada do log.
- `TaskOutput` — lê resultado e estado de tarefa de agente em segundo plano.
- `TaskStop` — solicita cancelamento cooperativo de tarefa em execução.
- `PowerShell` — **ADIADA**: este host Linux não dispõe de runtime PowerShell; não declarar execução funcional até que um host compatível esteja configurado.

**Agentes e workflows**

- `SendMessage` — envia mensagem de direção a um subagente em execução.
- `ListAgents` — lista apenas os subagentes da sessão atual.
- `SubagentHandback` — devolve relatório estruturado do subagente ao agente pai e encerra o turno filho.
- `Workflow` — executa workflow limitado com etapas paralelas; cada etapa herda somente as permissões permitidas ao pai.

**Planejamento e worktree**

- `EnterPlanMode` — ativa modo somente leitura para planejar.
- `ExitPlanMode` — apresenta o plano para aprovação e encerra o modo de planejamento.
- `EnterWorktree` — cria worktree Git isolada e restringe operações da sessão a ela.
- `ExitWorktree` — retorna ao workspace original sem apagar silenciosamente a worktree preservada.

**Agendamento**

- `CronCreate` — cria agendamento de uma execução ou recorrente, após aprovação.
- `CronDelete` — remove agendamento pertencente à sessão, após aprovação.
- `CronList` — lista agendamentos da sessão, próxima execução e resultado anterior.
- `ScheduleWakeup` — define ou limpa uma retomada futura somente de leitura; há no máximo uma pendente por sessão.
- `RemoteTrigger` — **ADIADA**: falta conector autenticado de rotina remota; não há execução remota afirmada.

**Web e extensões**

- `WebFetch` — obtém HTTPS público com validação de DNS/IP, bloqueio de redirecionamentos e limite de resposta.
- `WebSearch` — pesquisa resultados públicos e devolve títulos, URLs e trechos limitados.
- `ListMcpResourcesTool` — lista recursos de servidores MCP stdio explicitamente configurados.
- `ReadMcpResourceTool` — lê URI de recurso de servidor MCP configurado, em sandbox somente de leitura.
- `WaitForMcpServers` — verifica/inicializa servidores MCP configurados com timeout limitado.
- `LSP` — consulta servidor de linguagem explicitamente configurado e isolado para definições, referências e diagnósticos.

**Interação e entregáveis**

- `AskUserQuestion` — apresenta pergunta estruturada de múltipla escolha e aguarda resposta.
- `PushNotification` — envia notificação curta pelo HUD ativo; notificações do navegador respeitam a permissão do usuário.
- `SendUserFile` — oferece ao HUD arquivo existente no workspace; bloqueia traversal e arquivos acima de 25 MiB.
- `Artifact` — grava artefato de texto/HTML/JSON no workspace e o disponibiliza no HUD.
- `ReportFindings` — valida e devolve achados estruturados de revisão, com caminhos relativos ao workspace.
- `SendFeedback` — cria rascunho de feedback para revisão explícita; não o envia a serviço externo.
- `ShareOnboardingGuide` — gera guia de integração para download no HUD; compartilhamento externo exige provedor configurado.
- `EndConversation` — encerra o turno/sessão atual quando solicitado pelo usuário.

**Memória e edição de patch**

- `MemorySearch` — pesquisa memórias persistidas no escopo solicitado, com resultados limitados.
- `MemorySave` — salva fato explicitamente informado ou confirmado; rejeita segredos óbvios e credenciais.
- `MemoryForget` — apaga memória pelo ID e atualiza o índice de texto completo.
- `ApplyPatch` — aplica diff unificado dentro do workspace depois de leitura prévia dos arquivos-alvo.

### 8.4 Permissões e aprovações

As permissões são aplicadas pela política central (`policy.rs`) e pelo modo da sessão; esta lista é uma classificação de risco, não um mecanismo de autorização paralelo.

- **Leitura/contexto:** `Read`, `Glob`, `Grep`, `Monitor`, `TaskGet`, `TaskList`, `TaskOutput`, `ListAgents`, `ToolSearch`, `Skill`, `MemorySearch`, `WebFetch`, `WebSearch`, `ListMcpResourcesTool`, `ReadMcpResourceTool` e consultas `LSP`.
- **Escrita:** `Write`, `Edit`, `NotebookEdit`, `ApplyPatch`, `MemorySave` e `MemoryForget`.
- **Execução/orquestração:** `Bash`, `Agent`, `Workflow`, `TaskCreate`, `TaskUpdate`, `TaskStop`, `SendMessage`, `SubagentHandback` e `TodoWrite`.
- **Planejamento e repositório:** `EnterPlanMode`, `ExitPlanMode`, `EnterWorktree` e `ExitWorktree`; alterações no repositório nunca devem descartar silenciosamente trabalho não confirmado.
- **Agendamento/efeitos externos:** `CronCreate`, `CronDelete` e `ScheduleWakeup` exigem aprovação; `RemoteTrigger` permanece adiada.
- **Rede e recursos externos:** `WebFetch`, `WebSearch`, recursos MCP e LSP dependem de configuração explícita, limites e isolamento. As ferramentas de interação não afirmam envio externo quando só criam rascunhos ou entregáveis locais.

O modo `plan` deve bloquear mutações de arquivos, memória, tarefas e controle de agentes; o modo `ask` pede aprovação para efeitos colaterais conforme a política. Uma regra `deny` válida nunca pode ser enfraquecida por outra entrada inválida da política.

### 8.5 Garantias implementadas

- **Arquivos:** escrita atômica, leitura prévia para edição e checagem de caminhos dentro do workspace; `ApplyPatch` aceita diff limitado.
- **Tarefas e agentes:** isolamento por sessão, limites de concorrência, interseção de permissões e validação de dependências sem ciclos.
- **Agendamento:** persistência SQLite, aprovação na criação/remoção, limite de concorrência, execução somente de leitura e conjunto de ferramentas permitido explicitamente restrito.
- **Memória:** SQLite/FTS para busca, gravação e remoção persistentes; o estado degradado deve ser visível em saúde/diagnóstico, não mascarado como memória saudável.
- **MCP/LSP:** adaptadores implementados para servidores explicitamente configurados; os testes sintéticos verificam protocolo básico, não a disponibilidade de servidores de produção que não estejam instalados.
- **Sandbox:** perfil de execução com Landlock quando disponível e filtros seccomp; testes verificam negação de chamadas de rede, `io_uring` e tentativa de contorno pela ABI x32. A cobertura desses testes não substitui uma auditoria de kernel/LSM na máquina-alvo.
- **Saídas do HUD:** arquivos compartilhados como entregáveis são limitados ao workspace e ao tamanho máximo; feedback e guia de integração são rascunhos/arquivos locais, não envios externos.

### 8.6 Itens ainda adiados ou condicionados ao ambiente

- `PowerShell`: adiado neste host Linux por ausência de runtime compatível.
- `RemoteTrigger`: adiado até existir conector autenticado e allowlist de rotinas.
- `MCP` e `LSP`: despacho pronto, mas uso real depende de configuração de servidores e executáveis instalados.
- Voz, navegador, pesquisa de mídia e frame mobile: rotas do gateway podem encaminhar para serviços locais. Uma resposta HTTP não-404 comprova roteamento, não a funcionalidade completa do upstream.
- A chave OpenRouter e o modelo selecionado no HUD devem ser testados em staging sem imprimir nem copiar o segredo para logs. O teste deve usar o modelo selecionado pelo HUD e falhar sem fallback silencioso.

### 8.7 Evidência de testes e critérios de aceite

Estado da última suíte antes do build release: `cargo test --all-targets` aprovou 37 testes da biblioteca e 3 testes de conformidade de schemas; os outros alvos não declararam testes adicionais. `git diff --check`, a checagem de sintaxe dos scripts de deploy/teste e `py_compile` dos testes E2E passaram. A suíte inclui testes de política, persistência de agendamentos, isolamento por sessão, schemas, e bloqueios de syscalls da sandbox.

Isso **não** significa que todos os critérios de produção já passaram: a chamada real ao modelo, o conjunto E2E de rotas e o smoke test do binário release só devem ser marcados aprovados depois de executados em staging. O release só pode substituir o serviço ativo se health, limite de RAM, testes de rotas, teste sintético de API e chamada real ao modelo selecionado passarem; caso contrário, o script deve manter/restaurar a versão anterior.

## 9. Skills

Skills são pacotes de instruções e scripts que o agente carrega **só quando precisa**. Servem para encapsular métodos (como revisar código, como fazer um *commit* no estilo do projeto) sem inflar o prompt.

### 9.1 Formato e compatibilidade

- Pasta com `SKILL.md` (frontmatter YAML com `name` e `description`; opcionais `allowed-tools`, `when_to_use` e `version`) e subpastas `scripts/`, `references/` e `assets/`.
- Lê também `.claude/skills` e `.agents/skills`. Precedência: **projeto, usuário, plugin, embutida**.

### 9.2 Divulgação progressiva em três níveis

1. **Índice no prompt:** nome mais descrição curta (até 60 tokens cada), total de até 1,5 mil tokens. Passando de cerca de 40 skills, o índice mostra as mais relevantes por BM25 e o resto fica atrás de `skill_search`.
2. **`skill_load`:** traz o corpo do `SKILL.md` (limite de 5 mil tokens).
3. **Recursos e scripts:** lidos ou executados só quando o corpo da skill manda.

### 9.3 Execução e segurança

- Scripts rodam sempre pelo `bash` dentro do sandbox. As permissões efetivas são a **interseção** entre o que a skill declara e o que a sessão permite. Uma skill nunca eleva privilégio.
- Skills de terceiros: instalação com **hash fixado** no `plugins.lock`, exibição do conteúdo e das permissões pedidas, e bloqueio de padrões perigosos (`curl | sh`, envio de segredos para fora). Rede mais segredos exige confirmação explícita.
- O conteúdo de uma skill é tratado como instrução do usuário que a instalou, não como instrução do sistema. Nada em uma skill muda a política de permissões.

### 9.4 Ciclo de vida e qualidade

- Comandos: `harness skill new`, `validate`, `test`, `install`, `update` e `remove`.
- `validate` confere frontmatter, links e tamanho. `test` roda cenários do tipo “dado este pedido, a skill deve ser carregada e o resultado deve conter X” dentro do `hs-eval`.
- **Skills embutidas, poucas e boas:** `commit`, `review` (revisão de diff), `debug` (reproduzir, isolar, corrigir, validar), `refactor-safe`, `skill-creator` e `memory-curator` (revisão periódica da memória).
- Métricas: taxa de ativação correta e incorreta, e tokens gastos por skill.

## 10. Memória

A memória é o ponto onde o plano mais se distancia dos concorrentes: em vez de um arquivo de notas, um **sistema com camadas, busca híbrida, ciclo de vida e falhas visíveis**.

### 10.1 Camadas

1. **Trabalho:** o próprio contexto da conversa (seção 7).
2. **Episódica:** o log de eventos de cada sessão (JSONL). É a fonte bruta; nada é perdido.
3. **Semântica de longo prazo:** fatos, preferências, decisões e procedimentos extraídos das sessões, com ligações entre si (grafo).
4. **Procedural:** as skills e os arquivos de instruções do projeto.

### 10.2 Armazenamento: um único SQLite

```sql
CREATE TABLE memory (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,            -- fact | preference | decision | episode | procedure
  text TEXT NOT NULL,
  scope TEXT NOT NULL,           -- global | project:<hash> | session:<id>
  importance REAL DEFAULT 0.5,
  confidence REAL DEFAULT 0.7,
  created_at INTEGER, updated_at INTEGER, last_used_at INTEGER,
  use_count INTEGER DEFAULT 0,
  superseded_by INTEGER,         -- histórico em vez de apagar
  source TEXT,                   -- session:<id>#evento | user | import
  ttl INTEGER                    -- expiração opcional
);
CREATE VIRTUAL TABLE memory_fts USING fts5(text, content='memory', content_rowid='id');
CREATE TABLE memory_link (src INTEGER, dst INTEGER, rel TEXT, weight REAL);   -- grafo
CREATE TABLE memory_vec  (id INTEGER PRIMARY KEY, emb BLOB);                  -- opcional
CREATE TABLE memory_job  (id INTEGER PRIMARY KEY, kind TEXT, payload TEXT,
                          attempts INTEGER, next_at INTEGER, last_error TEXT);
```

- Modo WAL, `cache_size` de 64 MB, sem `mmap`. O índice FTS fica em disco.
- Um arquivo `MEMORY.md` legível por humanos é gerado por projeto a partir do banco, para o usuário poder ler e corrigir fora do harness.

### 10.3 Recuperação (leitura)

1. A consulta vem da tarefa atual (mensagem do usuário mais contexto recente). Reescrita por modelo barato é opcional.
2. **Candidatos:** BM25 do FTS5 (top 30), mais busca vetorial (top 30) se habilitada, mais expansão de um salto pelo grafo.
3. **Pontuação:** 0,5 relevância, 0,2 recência (decaimento exponencial, meia-vida de 30 dias por tipo), 0,2 importância e 0,1 frequência de uso.
4. **Diversidade (MMR)** para não repetir o mesmo fato com palavras diferentes.
5. **Injeção:** até 2 mil tokens no bloco de memória do prompt, cada item com identificador (`[m:123]`), para o modelo poder citar e o usuário poder auditar.

Meta de latência: até 30 ms sem vetores e até 80 ms com vetores.

### 10.4 Escrita

- **Explícita:** o modelo chama `memory_save`, ou o usuário diz “lembre que…”.
- **Reflexão em segundo plano:** ao fim da sessão (ou ao ficar ocioso), um modelo barato extrai candidatos a memória do log. Eles passam por uma fila com retentativas.
- **Deduplicação:** itens muito parecidos são fundidos. Contradições **substituem** o item antigo, mantendo o histórico por `superseded_by`.
- **Regra de proveniência:** só entra o que o usuário **disse** ou **confirmou**. Conclusões do próprio modelo ficam como candidatas até confirmação.
- **Filtro de segredos e de categorias sensíveis:** expressões regulares e entropia bloqueiam chaves, tokens e números de documento. Categorias sensíveis só entram com autorização explícita do usuário.

### 10.5 Falhas visíveis (lições do sistema anterior)

O sistema de memória que roda hoje acumulou mais de mil falhas de reflexão e um circuito aberto por chave inválida (erro 401), sem que isso ficasse óbvio para o usuário, e usava um índice só lexical, fraco para achar o que importa. O novo sistema trata isso como requisito:

- A fila (`memory_job`) tem retentativas com recuo, *dead-letter* e métricas de falha.
- **Se o provedor de memória falhar, o HUD mostra “memória degradada” com a causa.** Nada de queda silenciosa.
- O `harness doctor` e a partida do daemon **validam as chaves** antes de ligar a memória.
- O provedor da reflexão é configurado **separado** do modelo principal (um modelo barato dedicado).
- Nenhum *fallback* silencioso para uma qualidade pior.

### 10.6 Ciclo de vida e controle do usuário

- **Consolidação** (`memory_consolidate`): semanal ou a cada 500 itens novos. Funde quase-duplicatas, resume grupos em notas de nível mais alto e propõe podar o que é antigo, de baixa importância e nunca usado. Sempre com relatório de simulação para o usuário aprovar.
- **`/memory`:** listar, buscar, editar e apagar. **Esquecer é apagar de verdade:** remove o item, o vetor e roda `VACUUM`.
- Exportar e importar em JSONL. O que vem de outro assistente é tratado como **dado**, nunca como instrução.
- Escopos por projeto e global, para uma preferência de um projeto não vazar para outro.

### 10.7 Embeddings

- **Padrão: nenhum.** BM25 mais grafo já cobrem a maior parte e custam quase zero de RAM.
- **Opcional:** API de embeddings remota (em lote, com cache por hash do texto) ou um modelo local pequeno. Na máquina atual, o modelo local precisa ser compilado para a CPU base (sem AVX) e testado antes.
- Busca vetorial por força bruta é aceitável até cerca de 50 mil vetores. Acima disso, usar índice aproximado.

## 11. Plugins e hooks

### 11.1 Cinco tipos de plugin

1. **Embutidos em Rust**, ligados por *features* de compilação (git, LSP, navegador, memória, agendador). Rodam dentro do daemon, sem custo de IPC.
2. **Processos externos** que falam JSON-RPC por stdio. Qualquer linguagem. Se caírem, o daemon continua; têm teto de RAM por cgroup.
3. **Host Node com o Cordis real**, para plugins TypeScript (seção 4.5).
4. **Servidores MCP**, tratados como plugins com manifesto gerado automaticamente.
5. **WASM** (interpretador `wasmi`, de baixo consumo de RAM) para plugins puros e isolados. Fica para uma fase posterior.

### 11.2 Manifesto `plugin.toml`

```toml
name = 'git-extras'
version = '0.3.1'
runtime = 'process'          # builtin | process | node | mcp | wasm
entry = ['./bin/git-extras']
start = 'lazy'               # lazy | eager
memory_mb = 64               # teto (cgroup)
provides = ['git.stats']
inject = ['fs', 'memory']

[[tools]]
name = 'git_stats'
class = 'read'

[hooks]
on = ['post_tool_use']

[permissions]
fs_read = ['${project}']
net = []                     # sem rede
env = ['GIT_*']
```

O manifesto é a **fonte de verdade das permissões do plugin**. O daemon aplica o que está ali e nada além.

### 11.3 Protocolo JSON-RPC

- Métodos do daemon para o plugin: `plugin/initialize` (capacidades e configuração), `plugin/shutdown`, `tool/call`, `hook/event`.
- Métodos do plugin para o daemon: `service/call` (usar um serviço do harness, como memória), `log`, `progress`.
- `cancel` cooperativo para qualquer chamada em andamento, mais *heartbeat* a cada 10 s. Sem resposta em 3 batimentos, o plugin é considerado travado e reiniciado.
- Mensagens de até 1 MiB (conteúdo maior vai por arquivo). Versão do protocolo negociada na inicialização (semver).

### 11.4 Ciclo de vida

- Segue a máquina de estados da seção 4.3. A ordem de carga respeita as dependências (`inject`).
- Reinício com recuo exponencial (1, 2, 4... até 60 s). Depois de 5 falhas seguidas, o plugin vai para `Failed` e o HUD mostra o alerta com a causa.
- *Hot reload* sem reiniciar o daemon. Desativação automática por ociosidade para os plugins pesados.

### 11.5 Hooks

Pontos de gancho do barramento de eventos:

- `session_start`, `session_end`.
- `user_prompt_submit`: pode transformar ou bloquear o prompt (modo *waterfall* ou *bail*).
- `pre_tool_use`: permite, nega ou modifica a chamada (modo *bail*).
- `permission_request`: decide ou delega ao usuário.
- `post_tool_use`: observa e pode anexar contexto.
- `pre_compact`, `post_compact`.
- `subagent_start`, `subagent_stop`.
- `memory_write`: permite vetar ou editar uma memória antes de gravar.
- `stop`: roda ao fim do turno; pode pedir que o agente continue.

**Compatibilidade:** nomes, entrada em JSON pelo stdin e formato de decisão seguem o vocabulário que Claude Code e Codex já usam. Scripts de hook escritos para eles tendem a funcionar sem mudança, e o estudo de 11 harnesses indica que essa convergência já é a norma.

**Regras de falha:** hook de segurança que falha **nega** (*fail-closed*). Hook de observação que falha é ignorado, com registro (*fail-open*). Timeout padrão de 10 s.

### 11.6 Distribuição e confiança

- Instalação a partir de pasta local ou de repositório git com tag ou commit fixado. O `plugins.lock` guarda o hash.
- A instalação mostra as permissões pedidas. A atualização mostra o *diff* do manifesto e pede nova aprovação se as permissões crescerem.
- Plugins externos rodam com usuário dedicado sem privilégios, com o perfil de sandbox da seção 12. Assinatura de pacotes (minisign) entra numa fase futura.

### 11.7 Plugins de fábrica

`git`, `lsp` (gerencia os servidores), `browser`, `mcp-bridge`, `notify`, `scheduler` e `ocr` (opcional).

## 12. Permissões, sandbox e segurança

### 12.1 Quatro camadas

1. **Política declarativa** (`policy.toml`). Regras `allow`, `ask` e `deny` por ferramenta, combinadas com padrões de caminho, comando e domínio. Precedência: **deny, depois ask, depois allow**. Escopos: global, projeto e sessão (o mais específico vence, mas nunca pode afrouxar um `deny` de escopo mais amplo).
2. **Modos de aprovação:** plan, ask, auto-edit e auto (seção 6.4).
3. **Sandbox do sistema operacional.** Landlock para o sistema de arquivos e seccomp para chamadas de sistema, com `bubblewrap` opcional. Rede **desligada por padrão**, com um proxy local de lista de permissões. Limites de memória e de processos por cgroup. O Debian 13 traz um kernel com Landlock.
4. **Auditoria e reversão.** Log só de acréscimo, com hash encadeado, mais *snapshots* de arquivo e o comando `/rewind`.

O Codex CLI (Apache-2.0) tem uma pilha de segurança em quatro camadas e é a melhor referência pública para estudar e reaproveitar, com atribuição.

### 12.2 Exemplo de política

```toml
[defaults]
mode = 'ask'

[[rule]]
tool = 'bash'
match = 'git status*'
action = 'allow'

[[rule]]
tool = 'bash'
match = 'rm -rf *'
action = 'deny'

[[rule]]
tool = 'edit_file'
path = '${project}/**'
action = 'allow'

[[rule]]
tool = 'write_file'
path = '~/.ssh/**'
action = 'deny'

[[rule]]
tool = 'web_fetch'
domain = '*.docs.rs'
action = 'allow'
```

### 12.3 Injeção de prompt

- Tudo que vem de fora (páginas, arquivos, resultados de ferramentas, saída de plugin) é **marcado como não confiável** no prompt, entre delimitadores.
- Instruções que aparecem em conteúdo externo **nunca** alteram permissões, política ou lista de ferramentas.
- Ações sensíveis motivadas por conteúdo externo (por exemplo, enviar dados para uma URL que veio de uma página) exigem confirmação, mesmo em modo auto.
- Detecção por regras de ações suspeitas: exfiltração, comandos destrutivos e escrita fora do projeto.
- A saída das ferramentas passa por um filtro de segredos **antes** de ir ao modelo.

### 12.3.1 Segredos

- Cofre em arquivo `0600` (ou *keyring* do sistema). Referência por `${secret:NOME}`.
- Nunca em logs, transcritos nem na saída de ferramentas. O filtro de log remove tokens **e URLs que carregam tokens**, que é um problema real do stack atual, cujo log imprime a URL de sessão completa.

### 12.4 Modelo de ameaça resumido

- O usuário é confiável. O modelo é semiconfiável (pode errar ou ser manipulado). Conteúdo externo é hostil. Plugins e skills de terceiros são hostis até prova em contrário.
- O gateway escuta só em loopback e na Tailscale. **Nada de exposição pública (Funnel) sem autenticação forte.** O stack atual tem endereços públicos configurados para o HUD, e este plano não repete isso.

## 13. Orçamento de RAM: no máximo 1,5 GB

### 13.1 Princípio

- **Teto duro** por cgroup v2: `memory.max` = 1536 MB.
- **Freio suave:** `memory.high` = 1228 MB (80%). Passando disso, o kernel pressiona o grupo antes de matar qualquer coisa.
- **Watchdog interno** a 1,2 GB: dispara a liberação de memória descrita em 13.3.

### 13.2 Orçamento por componente (metas a validar na Fase 0)

**Fixo, sempre ligado: cerca de 350 MB**

- Daemon Rust: 120 MB típico (teto de 250).
- Cliente TUI: 25 MB.
- Cache do SQLite: 64 MB.
- Sessões ativas (até 8, cerca de 8 MB cada): 64 MB.
- Subagentes (até 6, cerca de 5 MB cada): 30 MB.
- Buffers de saída e caches LRU: 47 MB.

**Pool pesado, com controle de admissão: 800 MB**

- Navegador headless: até 450 MB, uma única instância.
- Servidor LSP: até 300 MB, um por vez.
- Host Node/Cordis: até 150 MB.
- Pool de execução do `bash` (todos os processos filhos somados): até 400 MB.
- Plugins externos: até 64 MB cada, no máximo 4 ao mesmo tempo.

A soma dos tetos passa de 800 MB de propósito: **nem tudo cabe ao mesmo tempo**, e o administrador de admissão decide o que fica ligado.

**Folga: cerca de 350 MB** para picos e para o *page cache* do kernel, que o cgroup contabiliza.

### 13.3 Controle de admissão

1. Cada componente pesado declara seu `memory_mb`.
2. Ao pedir ativação, o `hs-core` soma o uso atual (`memory.current` dos cgroups filhos).
3. Se couber, ativa.
4. Se não couber, **despeja o componente ocioso menos recente** (fecha o navegador, encerra o LSP, desliga o host Node).
5. Se ainda não couber, devolve ao modelo um erro acionável: “memória insuficiente para abrir o navegador; encerre o LSP ou use web\_fetch”.

### 13.4 Técnicas de Rust para ficar enxuto

- **Alocador:** comparar `mimalloc` com purga agressiva contra o alocador da glibc com `MALLOC_ARENA_MAX=2`, e ficar com o que medir menor. Alocadores com muitos *arenas* por thread inflam o RSS em máquinas de poucos núcleos.
- 2 threads de trabalho e canais sempre limitados.
- *Streaming* sem acumular: SSE e JSON-RPC analisados de forma incremental.
- `Bytes` e `Arc<str>` para não copiar; resultados grandes vão para disco e circulam como referência.
- Caches LRU com orçamento **em bytes**, não em número de itens.
- Skills, gramáticas tree-sitter e plugins carregados sob demanda.
- `serde_json::value::RawValue` onde o harness só repassa JSON sem inspecionar.
- Perfil de release: `lto = 'thin'`, `codegen-units = 1`, `panic = 'abort'`, `strip = true`; `opt-level = 3` no núcleo e `'s'` no cliente.

### 13.5 Medição e regressão

- `harness doctor --mem` lê `VmRSS` do `/proc/self/status` e `memory.current` do cgroup, por componente.
- Endpoint `/metrics` com RSS, tokens, filas e erros, mostrado no HUD.
- **Teste de regressão no CI:** cenário padrão (sessão de 200 turnos com 40 chamadas de ferramenta) que falha se o pico passar do orçamento.
- **Teste de vazamento:** sessão de 8 horas com crescimento de RSS de no máximo 10%.

### 13.6 A realidade da máquina atual

- O PC tem cerca de 1,95 GB físicos. Um teto de 1,5 GB só cabe se o resto (SSH, Tailscale, mobile agent, servidor de chamada) ficar abaixo de \~400 MB. Por isso a meta de uso típico é de **até 600 MB**, e o teto é só a rede de segurança.
- Existe swap de 8 GB, mas swap ativo derruba a latência. O watchdog trata uso de swap pelo cgroup como alerta.

### 13.7 Compilação

- Compilar Rust nesse Atom com 2 GB seria demorado e arriscado (falta de memória na ligação). **Compile em outra máquina ou em CI** para `x86_64-unknown-linux-gnu` (ou `musl`, para binário estático) com `-C target-cpu=x86-64` (baseline).
- O Atom N2600 não tem SSE4 nem AVX (conferir em `/proc/cpuinfo`). Evitar crates e binários pré-compilados que exijam instruções mais novas. SQLite e tree-sitter compilam bem na base.
- Binário esperado entre 15 e 25 MB (estimativa; medir na Fase 0).

## 14. Como competir com Codex e Claude Code: avaliação

### 14.1 Princípio

Comparar sempre com o **mesmo modelo** nos três harnesses. Só assim a diferença medida é do harness, e não do modelo. Cada comparação roda várias vezes (no mínimo 3) e reporta média e intervalo, porque modelos têm variância.

### 14.2 Suítes de avaliação

1. **Edição (interna, 300 tarefas):** arquivos grandes, indentação mista, finais de linha CRLF, Unicode, patches de vários arquivos, trechos repetidos. Mede sucesso na primeira tentativa e tentativas por tarefa. É onde o `edit_file` e o `apply_patch` provam seu valor.
2. **Engenharia de software:** um subconjunto de um benchmark público de correção de issues (por exemplo, SWE-bench Verified), o Terminal-Bench e o benchmark poliglota do Aider. Confirmar disponibilidade e licença de cada um na hora. Os que usam container pesado rodam fora do PC atual.
3. **Sessões longas (200 turnos ou mais):** depois de várias compactações, o agente responde perguntas sobre decisões tomadas há 100 turnos. Mede perda de informação.
4. **Memória entre sessões:** preferências e decisões ditas em uma sessão devem aparecer corretamente em outra, sem alucinação.
5. **Segurança:** páginas e arquivos com instruções maliciosas (injeção de prompt) e tentativas de fuga de sandbox.
6. **Recursos:** pico de RSS, CPU, tempo de relógio, tokens e custo por tarefa.

### 14.3 Placar

Sucesso (%), tokens totais, custo em dólares, tempo, chamadas de ferramenta com erro, pico de RSS, acertos de cache de prompt e número de compactações.

### 14.4 Metas de paridade e de superioridade

- **Fase 2:** paridade de edição com o melhor concorrente no benchmark interno.
- **Fase 5:** diferença de no máximo 5 pontos percentuais de sucesso nas tarefas de engenharia com o mesmo modelo.
- **Superioridade esperada (a provar):** pico de RSS, tokens por tarefa, memória entre sessões e custo.

### 14.5 Cuidados

- Benchmarks públicos vazam para o treino dos modelos. Manter um conjunto privado de tarefas.
- Nunca ajustar o harness às tarefas do conjunto de teste. Separar tarefas de desenvolvimento e de validação.
- Um bom resultado de custo e RAM não compensa queda de sucesso. O placar é lido junto.

## 15. Stack Rust e dependências

**Observação importante:** os nomes abaixo são as bibliotecas que eu usaria, mas **não verifiquei as versões atuais**. Na Fase 0, consultar o crates.io e usar as últimas estáveis, que é o que o pedido de “melhores versões” pede.

- **Assíncrono e rede:** `tokio` (rt, net, sync, time, process), `tokio-util` (`CancellationToken`), `axum` e `tower` (gateway), `reqwest` com `rustls` (provedores de modelo). Avaliar um cliente HTTP mais enxuto se o binário ficar grande.
- **Dados:** `serde`, `serde_json`, `schemars` (esquemas das ferramentas), `toml` e `figment` (configuração), `bytes`.
- **Erros e observabilidade:** `thiserror`, `anyhow`, `tracing` e `tracing-subscriber` (JSON).
- **CLI e TUI:** `clap`, `ratatui` e `crossterm`.
- **Armazenamento:** `rusqlite` (SQLite embutido com FTS5).
- **Busca e arquivos:** `ignore`, `globset`, `grep-regex` e `grep-searcher` (as bibliotecas do ripgrep), `notify` (inotify), `similar` (diffs).
- **Código:** `tree-sitter` com gramáticas carregadas sob demanda, `lsp-types` com cliente LSP próprio sobre stdio.
- **Execução e isolamento:** `portable-pty`, `landlock`, `seccompiler`.
- **Navegador:** cliente CDP mínimo próprio sobre `tokio-tungstenite`, para evitar uma dependência grande.
- **Opcionais:** `wasmi` (plugins WASM), `keyring`, `mimalloc`.

**Política de dependências:**

- Cada dependência nova precisa de justificativa (o que ela evita escrever e quanto pesa no binário e na compilação).
- `cargo-deny` (licenças e avisos de segurança), `cargo-audit`, `cargo-bloat` e `cargo-udeps` no CI.
- `Cargo.lock` versionado e MSRV (versão mínima do compilador) declarada.

## 16. Observabilidade e testes

### 16.1 Observabilidade

- `tracing` com *spans* por turno e por ferramenta. Log em JSONL com rotação e filtro de segredos.
- **Métricas:** tokens de entrada, saída e de cache; latência por fase; RSS por componente; fila da memória; erros por ferramenta.
- **Replay:** qualquer sessão pode ser reproduzida a partir do log.
- **`harness doctor`:** valida chaves, rede, portas, cgroup, versões e o estado dos plugins.
- **Painel no HUD:** RAM por componente, tokens, falhas e alertas visíveis (“memória degradada”, “plugin caído”, “RAM em 80%”).

### 16.2 Testes

- **Unitários e de referência (*golden*)** para cada ferramenta.
- **Propriedade** (`proptest`) para `edit_file`, `apply_patch` e o ciclo de vida de plugins.
- **Fuzzing** (`cargo-fuzz`) no analisador de patch, no SSE, no JSON-RPC e no tratamento de caminhos.
- **Integração com servidor de modelo simulado:** respostas gravadas e reproduzidas (“cassetes”) para testar o loop de forma determinística.
- **Sandbox:** tentativas de ler `~/.ssh`, escrever fora do projeto e abrir conexões de rede.
- **Caos:** matar um plugin, matar o daemon no meio de uma escrita (a atomicidade e a retomada precisam sobreviver).
- **Soak de 8 horas** com acompanhamento de RSS.
- **Compatibilidade:** uma suíte de hooks e de skills de exemplo vindos de outros harnesses.
- **CI:** matriz mínima em Linux x86\_64 e um job dedicado de RAM.

## 17. Roadmap por fases

As semanas assumem 15 a 20 horas semanais de uma pessoa. Se for menos, multiplique. **Cada fase termina com uma demonstração e com números medidos.**

### Fase 0: decisões e *spikes* (semana 1)

- Loop mínimo em Rust com *streaming* de um provedor, medindo RSS.
- `cordis-rs` mínimo: `Ctx`, `Plugin` e eventos.
- `grep` em processo e `edit_file` com 20 casos de teste.
- Compilação cruzada para a máquina-alvo funcionando.
- **Saída:** relatório com números reais de RAM e decisão de seguir ou ajustar o plano.

### Fase 1: MVP (semanas 2 a 8)

- Daemon, 1 provedor (com a interface pronta para outros), loop do agente, as 12 ferramentas do núcleo, política de permissões simples, sessões em JSONL, TUI mínima, cgroup.
- **Critério:** executar uma tarefa real de edição num repositório, com RSS típico de até 300 MB.

### Fase 2: contexto, skills e hooks (semanas 9 a 14)

- Compactação em camadas, `AGENTS.md`, skills (os três níveis), hooks, subagentes, modos plan, ask, auto-edit e auto, `/rewind`.
- **Critério:** sessão de 200 turnos sem estourar o contexto e paridade de edição no benchmark interno.

### Fase 3: memória e plugins (semanas 15 a 20)

- Memória (SQLite, FTS5, grafo, fila, interface `/memory`), host de plugins por JSON-RPC, cliente MCP, host Node com Cordis, `tool_search`.
- **Critério:** a memória recupera preferências entre sessões, e um plugin TypeScript de exemplo roda.

### Fase 4: catálogo completo (semanas 21 a 28)

- As 48 ferramentas restantes (código e LSP, git, web e navegador, documentos, agendamento) e o controle de admissão de RAM.
- **Critério:** catálogo com testes e pico de RSS de até 1,5 GB no cenário pesado.

### Fase 5: endurecimento e avaliação (semanas 29 a 34)

- Sandbox completo (Landlock, seccomp, rede), red-team, comparativos com o mesmo modelo, otimização de RAM e latência.
- **Critério:** metas da seção 3 atingidas.

### Fase 6: gateway, HUD e produção (semanas 35 a 40)

- Gateway completo, HUD como cliente, ACP para IDE, empacotamento como serviço runit, migração do stack atual com *rollback*, documentação.
- **Critério:** o stack atual substituído em produção, com caminho de volta testado.

## 18. Riscos e mitigações

1. **Cordis não é Rust, e a reimplementação pode crescer.** Limitar o `cordis-rs` ao essencial (Ctx, Fiber, Services, Events), em torno de 2 a 3 mil linhas. Congelar a API e cobrir o ciclo de vida com testes de propriedade.
2. **Estouro de RAM por LSP e navegador.** Controle de admissão, cgroup, plugins sob demanda e métricas.
3. **Compilação na máquina fraca.** Compilar em outra máquina ou em CI e versionar os binários.
4. **CPU sem AVX incompatível com algum binário.** Teste de fumaça na máquina-alvo no CI e checagem no `harness doctor`.
5. **Catálogo de ferramentas.** O catálogo verificado tem 50 nomes (48 prontos e 2 adiados); expansão só entra com testes e cumprindo o padrão de qualidade da seção 8.1.
6. **Diferenças entre provedores** (formatos de chamada de ferramenta). Camada `hs-model` com testes de contrato e gravações por provedor.
7. **Injeção de prompt e plugins hostis.** Seção 12, com permissões declaradas e aprovação na instalação.
8. **Dependência de uma só pessoa.** Documentação, registros de decisão (ADRs) e testes como especificação.
9. **Concorrentes evoluem rápido.** Manter a compatibilidade (hooks, skills, `AGENTS.md`, MCP, ACP) e focar nos diferenciais medidos.
10. **Licenças.** Adotar Apache-2.0, auditar qualquer reaproveitamento do Codex (atribuição e arquivo NOTICE) e não copiar código proprietário.

## 19. Decisões em aberto (preciso da sua resposta)

1. **Ferramentas:** o catálogo atual tem 50 nomes, 48 prontos e 2 adiados. Confirmar se a paridade exata com as 46 ferramentas do Claude Code continua sendo objetivo separado; não presumir equivalência sem mapeamento por ferramenta. O teto de 1,5 GB vale para o harness e processos filhos, sem modelo local dentro do orçamento.
2. **Provedores:** quais primeiro? Sugestão: Gemini e DeepSeek (os do seu stack atual) mais um provedor compatível com a API da OpenAI.
3. **Onde compilar?** Outra máquina sua ou um serviço de CI na nuvem?
4. **Nome definitivo e licença** (sugestão: Apache-2.0).
5. **HUD e celular:** entram só na Fase 6 ou antes? Um cliente web cedo permite testar do celular desde a Fase 1.
6. **Memória:** vetores desde o início ou só BM25 mais grafo (sugestão: só BM25 e grafo no começo)?
7. **Reaproveitar código do Codex (Apache-2.0)** onde fizer sentido, ou escrever tudo do zero?
8. **O harness em C++ que você já começou:** este plano o substitui ou os dois convivem?

### Próximo passo concreto

Se você aprovar, o primeiro passo é a **Fase 0**: criar o workspace, fazer os quatro *spikes* da seção 17 e voltar com um relatório de RAM e de compilação cruzada. É uma semana de trabalho e responde à pergunta que mais pode mudar o plano: quanto o núcleo em Rust realmente consome na sua máquina.