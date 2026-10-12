<div align="center">

# 🧠 Thomas · Harness-RS

### Mais que conversar. A meta é agir, acompanhar e aprender com a experiência.

**Um runtime de agente em Rust para construir um assistente pessoal capaz de usar ferramentas, manter contexto e avançar em tarefas longas — com memória, aprovações e limites de segurança.**

[![CI](https://github.com/sl7187855-coder/harness-rs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/sl7187855-coder/harness-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-orange?logo=rust)
![Platform](https://img.shields.io/badge/target-Linux%20x86--64-lightgrey?logo=linux)
![Help wanted](https://img.shields.io/badge/contributions-welcome-brightgreen.svg)

**[🚀 Código](https://github.com/sl7187855-coder/harness-rs) · [📱 Thomas Mobile Agent 0.5.3](https://github.com/sl7187855-coder/harness-rs/releases/tag/mobile-agent-v0.5.3) · [🤝 Contribua](https://github.com/sl7187855-coder/harness-rs/issues/1) · [🟢 Tarefas para iniciantes](https://github.com/sl7187855-coder/harness-rs/labels/good%20first%20issue)**

</div>

---

## 🌟 O que é o Thomas?

O **Thomas** é a visão de um assistente-agente pessoal: algo que não apenas responde, mas consegue entender um objetivo, quebrá-lo em etapas, usar ferramentas, acompanhar o que já fez, verificar resultados e continuar trabalhando em tarefas demoradas.

O **Harness-RS** é a base de execução que estamos construindo para essa visão. Ele combina runtime Rust, gateway HTTP/SSE para o HUD, ferramentas, contexto, skills, memória e estado persistente em SQLite, tarefas/agendas, políticas de aprovação e isolamento de subprocessos no Linux.

> 🎯 **Nosso objetivo:** um agente útil e persistente que consiga levar tarefas mais longe, preservar lições importantes de tentativas anteriores e melhorar através de resultados registrados e feedback — sem esconder erros, inventar sucesso ou ignorar aprovações.

**Sobre “aprender”:** nesta etapa significa usar memória, resultados e feedback para orientar ações futuras. Não significa que o modelo de IA treine seus próprios pesos automaticamente. Memória avançada, consolidação e outras capacidades de longo prazo continuam em desenvolvimento.

## 🥔 Nosso laboratório: um PC batata, de verdade

O projeto é mantido numa máquina Positivo antiga, da classe netbook, com recursos muito apertados. Estas especificações foram lidas diretamente do sistema durante a preparação do repositório:

| Máquina real | Especificação observada |
|---|---|
| Fabricante / modelo reportado | Positivo Informática · EC10IS1 |
| Processador | Intel Atom N2600 @ 1,60 GHz |
| CPU | 2 núcleos físicos / 4 threads |
| Memória RAM detectada | Aproximadamente 2 GiB (cerca de 1,9 GiB apresentados pelo Linux) |
| Data reportada pelo firmware/BIOS | `04/11/2014` — campo do firmware, não a data confirmada de fabricação |
| Sistema | antiX / Debian 13 · Linux x86-64 |

Essa plataforma já é bem antiga para desenvolvimento moderno. Builds otimizados podem levar bastante tempo, a pressão de memória limita testes paralelos e testes de muitas horas são difíceis de executar. Não temos uma estação de trabalho parruda escondida nos bastidores; tentamos medir, testar e documentar com os recursos que temos.

**Por isso precisamos de você.** Quem tem hardware mais forte, outro Linux, um Android para testar ou experiência em Rust, segurança, testes e agentes pode fazer uma diferença enorme.

## 🧭 O que queremos que o Thomas seja capaz de fazer

- **Trabalhar em tarefas longas:** dividir objetivos em passos, acompanhar progresso, recuperar estado e retomar sem depender de uma única resposta.
- **Manter contexto e memória útil:** guardar informações relevantes e recuperar experiências anteriores para reduzir repetição e perda de contexto.
- **Aprender com resultados:** reconhecer falhas, registrar o que funcionou e adaptar novas tentativas com base em memória e feedback.
- **Usar ferramentas e skills:** atuar sobre arquivos, terminal e integrações com descoberta, esquemas e regras de permissão.
- **Ter autonomia com freios:** automatizar quando permitido e pedir aprovação nas ações que exigem controle.
- **Ser auditável e seguro:** limitar permissões, registrar eventos e falhar de forma segura quando controles críticos não estão disponíveis.
- **Evoluir por extensões:** ampliar integração de modelos/provedores, MCP, LSP, plugins e o agente móvel.
- **Rodar em hardware modesto:** reduzir uso de memória e tornar o desenvolvimento viável em máquinas fracas.

São metas de desenvolvimento, não a alegação de que toda a visão já está completa. Veja as lacunas conhecidas em [PROJECT_STATUS.md](docs/PROJECT_STATUS.md).

## 🚦 Estado atual: early alpha

A CI do GitHub passou nos dois jobs públicos mais recentes verificados: testes Rust, Clippy, build release Linux x86-64, verificações de scripts/HUD, relay/MCP e compilação do Android. Isso valida aquele commit e aquelas verificações; não significa que todos os recursos foram testados em todos os ambientes.

| Área | Situação atual |
|---|---|
| Runtime do agente e streaming de provedor | Implementados; falta ampliar cobertura por provedor |
| Gateway HTTP/SSE e HUD | Implementados; autenticação/cookies do gateway ainda são uma lacuna importante |
| Catálogo de ferramentas | 50 nomes; 48 disponíveis para despacho e 2 adiados |
| Ferramentas de arquivo / Bash | Implementadas, com modos de aprovação e snapshots para operações suportadas |
| Sandbox Linux | Landlock/seccomp em caminhos suportados; precisa de mais revisão independente |
| Contexto e skills | Carregamento de arquivos de instrução suportados e skills locais |
| Memória e tarefas persistentes | SQLite para mensagens, memória, tarefas e agendas |
| Auditoria | Registros JSONL com encadeamento SHA-256; não protege contra administrador que reescreva tudo |
| MCP | Subconjunto limitado de recursos via stdio; não é cobertura completa do protocolo |
| LSP | Adaptador JSON-RPC stdio testado com servidores simulados; integração real precisa de mais testes |
| Host completo TypeScript/Cordis | Ainda não implementado ponta a ponta |
| PowerShell / RemoteTrigger | Adiados neste estágio |

**50 nomes no catálogo não significam 50 ferramentas certificadas de forma independente.** WebSearch ainda precisa de resultados positivos úteis verificados; WebFetch, handoff entre subagentes, integração com servidores reais e workloads longos também têm lacunas. O projeto não reivindica paridade completa com Codex CLI ou Claude Code.

Detalhes: [estado e prioridades](docs/PROJECT_STATUS.md) · [conformidade e evidências](docs/CONFORMANCE.md) · [arquitetura](docs/ARCHITECTURE.md) · [segurança](SECURITY.md) · [plano original](docs/PROJECT_PLAN_ORIGINAL.md).

## ⚡ Quick Start — instalar e executar

> **Nota:** o build inicial pode demorar bastante em máquinas antigas. Execute uma etapa por vez, use apenas ferramentas que você confia e mantenha o gateway em loopback. Ainda não exponha a API de execução à internet.

### 1. Instale os pré-requisitos

No Linux baseado em Debian/Ubuntu, instale as ferramentas básicas:

~~~sh
sudo apt update
sudo apt install -y build-essential pkg-config curl git python3 nodejs npm
~~~

Instale Rust estável seguindo as instruções oficiais em [rustup.rs](https://rustup.rs/). Depois, abra um novo terminal ou carregue o ambiente Rust e confirme as ferramentas:

~~~sh
rustup update stable
rustup component add rustfmt clippy
rustc --version
cargo --version
~~~

É necessário Rust **1.88 ou superior** (edition 2024). Algumas dependências Cargo podem exigir bibliotecas de desenvolvimento adicionais específicas da distribuição.

### 2. Baixe o código

~~~sh
git clone https://github.com/sl7187855-coder/harness-rs.git
cd harness-rs
~~~

### 3. Verifique e compile

~~~sh
cargo fmt --check
CARGO_BUILD_JOBS=1 cargo test --locked --all-targets
CARGO_BUILD_JOBS=1 cargo build --locked --release
~~~

Em um computador com pouca RAM, mantenha `CARGO_BUILD_JOBS=1`. Para rodar as verificações extras usadas no CI:

~~~sh
CARGO_BUILD_JOBS=1 cargo clippy --locked --all-targets
python3 -m py_compile tests/e2e/*.py scripts/check_inline_js.py
bash -n scripts/deploy.sh
sh -n scripts/test.sh
python3 scripts/check_inline_js.py web/thomas-hud.html
~~~

### 4. Inicie localmente

Na raiz do repositório, prepare diretórios isolados e execute o runtime:

~~~sh
mkdir -p .local-workspace .local-home/data
export HARNESS_HOME="$PWD/.local-home"
export HARNESS_DATA="$PWD/.local-home/data"
export HARNESS_WORKSPACE="$PWD/.local-workspace"
export HARNESS_BIND="127.0.0.1:1347"
export HARNESS_BRIDGE_BIND="127.0.0.1:3196"
export HARNESS_HUD_FILE="$PWD/web/thomas-hud.html"
export HARNESS_EXECUTION_MODE="ask"
cargo run --release
~~~

Abra [http://127.0.0.1:1347/](http://127.0.0.1:1347/) no próprio computador. Configure um provedor compatível pela interface de configurações, onde suportado, ou configure as variáveis privadas de ambiente antes de iniciar o runtime:

~~~sh
export HARNESS_API_BASE_URL="URL_BASE_DO_SEU_PROVEDOR"
export HARNESS_MODEL="ID_DO_MODELO"
read -r -s -p "API key: " HARNESS_API_KEY; printf '\n'
export HARNESS_API_KEY
~~~

Troque os placeholders pelos valores do provedor que você escolheu. O projeto **não carrega `.env` automaticamente**. A variável com a chave deve permanecer apenas no ambiente privado: nunca a coloque no código, no histórico do shell, em capturas de tela, issues ou commits. Use apenas credenciais de teste descartáveis para validar integrações.

Se o runtime não iniciar como esperado, consulte [CONFIGURATION.md](docs/CONFIGURATION.md), [PROJECT_STATUS.md](docs/PROJECT_STATUS.md) e abra uma [issue com passos para reproduzir](https://github.com/sl7187855-coder/harness-rs/issues/new/choose), sem incluir segredos.

## 🏗️ Arquitetura em um olhar

~~~mermaid
flowchart TD
  HUD["HUD / interface"] <-->|"HTTP + SSE"| GW["Gateway Rust"]
  GW <--> AG["Loop do agente + provedor"]
  AG --> TOOLS["Ferramentas e schemas"]
  AG --> CTX["Contexto · skills · hooks"]
  AG --> POLICY["Política e aprovação"]
  POLICY --> SANDBOX["Execução isolada"]
  SANDBOX --> OS["Arquivos · Bash · MCP · LSP"]
  AG <--> DB["SQLite: memória · tarefas · histórico"]
  SANDBOX --> AUDIT["Auditoria encadeada"]
  AG <-->|"API compatível"| MODEL["Modelo configurado"]
~~~

## 📱 Thomas Mobile Agent — Android

O repositório inclui o app Android, relay, CLI e adaptador MCP. A versão **0.5.3 (versionCode 12)** tem código-fonte público e APK de teste com assinatura de depuração.

- [📲 Baixar APK 0.5.3](https://github.com/sl718785-coder/harness-rs/releases/download/mobile-agent-v0.5.3/Thomas-Mobile-Agent-0.5.3-debug.apk)
- [🧾 Release, SHA-256 e notas](https://github.com/sl718785-coder/harness-rs/releases/tag/mobile-agent-v0.5.3)
- [📖 Documentação móvel](mobile/README.md)

A compilação Android foi aprovada na CI, mas instalação, login e conectividade ainda precisam ser retestados em um aparelho físico. O APK é debug-signed, não é um build de produção/Play Store. Acessibilidade e captura de tela exigem consentimento explícito no dispositivo.

## 🔐 Segurança primeiro

- Mantenha o gateway em `127.0.0.1` ou em rede privada confiável até a autenticação de sessão ser implementada e revisada independentemente.
- Não exponha o endpoint de execução à internet e não use túnel público como substituto de autenticação.
- Use modo `ask` e não execute o agente em workspace não confiável com modo automático.
- Trate arquivos de instrução, skills, hooks, ferramentas MCP/LSP e resultados externos como entradas que precisam ser revisadas.
- Nunca versione `.env`, chaves, tokens, perfis privados, banco de dados, logs, certificados ou backups.
- Sandbox é uma camada de defesa, não uma garantia absoluta.

Leia [SECURITY.md](SECURITY.md) antes de reportar vulnerabilidades.

## 🤝 Contribuições: vamos construir isso juntos

**Eu vou continuar lançando atualizações para o Thomas**, corrigindo bugs e ampliando a capacidade do agente. Mas o desenvolvimento acontece em uma máquina Positivo antiga com Atom N2600 e cerca de 2 GiB de RAM. Isso limita o número de testes longos, integrações e dispositivos que conseguimos verificar pessoalmente.

Precisamos de ajuda de quem possa revisar código, executar builds em hardware diferente, testar o Android, cobrir integrações, melhorar docs e desenvolver lacunas conhecidas.

### 🌱 Boas primeiras tarefas

- [Ver todas as issues `good first issue`](https://github.com/sl7187855-coder/harness-rs/labels/good%20first%20issue)
- [Ver a chamada geral para colaboradores](https://github.com/sl7187855-coder/harness-rs/issues/1)
- [Abrir uma nova issue](https://github.com/sl7187855-coder/harness-rs/issues/new/choose)

Veja o [CONTRIBUTING.md](CONTRIBUTING.md) para instalação, testes e regras de PR. Contribuições pequenas também contam: reproduzir um bug, testar em outra máquina, corrigir um exemplo ou melhorar uma instrução pode ajudar bastante.

### 🏆 Agradecimentos aos contribuidores

Obrigado a toda pessoa que dedicar tempo a revisar, documentar, testar ou melhorar o Thomas. Os nomes e contribuições públicas serão listados automaticamente pelo histórico do GitHub:

[![Contributors](https://contrib.rocks/image?repo=sl7187855-coder/harness-rs)](https://github.com/sl7187855-coder/harness-rs/graphs/contributors)

[Ver o gráfico de contribuidores](https://github.com/sl7187855-coder/harness-rs/graphs/contributors) · [Ver pull requests](https://github.com/sl7187855-coder/harness-rs/pulls)

---

<div align="center">

### ❤️ Uma ideia ambiciosa, um PC batata e muita coisa para construir.

**Thomas quer se tornar um assistente-agente persistente, útil e confiável. Ajude a gente a chegar lá.**

[⭐ Harness-RS](https://github.com/sl7187855-coder/harness-rs) · [🤝 Contribuir](https://github.com/sl7187855-coder/harness-rs/issues/1) · [📱 Aplicativo Android](https://github.com/sl7187855-coder/harness-rs/releases/tag/mobile-agent-v0.5.3)

</div>

---

### Integridade do APK Thomas Mobile Agent 0.5.3

SHA-256 do APK de teste: `625a5a030981a61ca4a23503c3741b9e5cd0d2399950beeb1ea2e3a02af3333b`. Compare com o arquivo `.sha256` da release antes de instalar.
