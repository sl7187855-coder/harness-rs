# 🤝 Contribuindo com o Thomas: agente + Harness-RS + HUD Web + Android

Obrigado por querer ajudar! **Thomas é o projeto integrado; Harness-RS é seu runtime em Rust.** O conjunto também inclui a HUD Web e o Thomas Mobile Agent para Android. A visão é um assistente-agente que consiga conduzir tarefas longas, usar ferramentas, persistir contexto e memória, verificar resultados e melhorar a partir de resultados registrados e feedback. O projeto está em early alpha: algumas integrações e capacidades ainda precisam de testes, revisão e implementação.

## 🧭 Mapa rápido do projeto

- **Núcleo / runtime:** [src/](src) — loop do agente, ferramentas, políticas e gateway.
- **HUD Web:** [web/thomas-hud.html](web/thomas-hud.html) — interface do runtime.
- **Companheiro Android:** [mobile/README.md](mobile/README.md) — app e instruções de build/teste.
- **Estado, lacunas e arquitetura:** [PROJECT_STATUS.md](docs/PROJECT_STATUS.md) · [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 🥔 Por que a sua ajuda é importante

O mantenedor desenvolve numa máquina Positivo EC10IS1 antiga, com Intel Atom N2600 @ 1,60 GHz, 2 núcleos/4 threads e cerca de 2 GiB de RAM. Builds de release e testes prolongados demoram e não é possível cobrir sozinho todos os provedores, distribuições Linux, integrações e aparelhos Android. Relatos reproduzíveis de outras máquinas são especialmente valiosos.

**Vou continuar publicando correções e atualizações; a contribuição da comunidade ajuda a acelerar a qualidade, a cobertura e a segurança.**

## 🚀 Quick Start para desenvolvimento

1. Instale Git, Rust estável (1.88+ / edition 2024), `rustfmt`, `clippy`, um toolchain C e as dependências de sistema exigidas pelo Cargo. Para as verificações do HUD, instale Python 3 e Node.js.
2. Clone o repositório e entre na pasta:

   ~~~sh
   git clone https://github.com/sl7187855-coder/harness-rs.git
   cd harness-rs
   rustup update stable
   rustup component add rustfmt clippy
   ~~~

3. Execute as verificações básicas em sequência. Em máquinas fracas, use um job de build por vez:

   ~~~sh
   cargo fmt --check
   CARGO_BUILD_JOBS=1 cargo test --locked --all-targets
   CARGO_BUILD_JOBS=1 cargo clippy --locked --all-targets
   python3 -m py_compile tests/e2e/*.py scripts/check_inline_js.py
   bash -n scripts/deploy.sh
   sh -n scripts/test.sh
   python3 scripts/check_inline_js.py web/thomas-hud.html
   ~~~

4. Para iniciar o HUD local, use a seção **Quick Start** do [README principal](README.md). Configure credenciais de provedor no seu ambiente privado; elas não devem ir para o Git.
5. Consulte [PROJECT_STATUS.md](docs/PROJECT_STATUS.md), [CONFORMANCE.md](docs/CONFORMANCE.md) e as issues existentes antes de começar.

## 🌱 Primeiras contribuições

Procure issues com a etiqueta [`good first issue`](https://github.com/sl718785-coder/harness-rs/labels/good%20first%20issue). São tarefas pequenas e delimitadas para ajudar quem está começando. Se nenhuma servir, comente na [issue de chamada à comunidade](https://github.com/sl718785-coder/harness-rs/issues/1) ou abra uma nova issue descrevendo a ideia.

Áreas prioritárias:

- Documentação de instalação, exemplos e solução de problemas.
- Testes determinísticos sem chave API, usando mocks e pastas temporárias.
- Builds e regressões em outras versões do Linux / hardware x86-64.
- Segurança do gateway, aprovações, sandbox e auditoria.
- Testes de ferramentas/provedores, MCP/LSP e validação do Android em dispositivos reais.
- Benchmarks reproduzíveis de RAM, tempo de compilação e workloads longos.

## ✅ Regras para issues e pull requests

1. **Procure antes:** verifique issues e PRs existentes para evitar trabalho duplicado.
2. **Explique a mudança:** descreva o problema, a solução e o comportamento esperado.
3. **Teste quando possível:** adicione ou atualize testes. Inclua os comandos executados e os resultados reais.
4. **Diga o tipo de evidência:** informe se testou com mock, provedor real de staging, aparelho físico ou apenas revisão de código.
5. **PRs pequenos são melhores:** uma mudança focada é mais simples de rever. Atualize docs quando comportamento ou comandos mudarem.
6. **Não enfraqueça a segurança:** não remova aprovações, isolamento ou fail-closed para fazer uma verificação passar.
7. **Se não puder rodar tudo, diga isso:** logs claros e limitados são melhores do que afirmar que testes não executados passaram.
8. **Não alegue paridade total:** a existência de um nome no catálogo não significa conformidade funcional independente.

## 🔐 Segredos e dados pessoais

Nunca envie em issues ou PRs:

- Chaves API, tokens, senhas ou cookies.
- IPs/URLs privados, chaves privadas, certificados de produção ou arquivos de autenticação.
- Bancos SQLite locais, históricos pessoais, logs sem sanitização ou dados identificáveis de dispositivos.
- Arquivos `.env`, settings privados, builds locais e backups.

Vulnerabilidades devem ser relatadas conforme o procedimento privado em [SECURITY.md](SECURITY.md), não numa issue pública. Não exponha o gateway de execução à internet sem autenticação e revisão de segurança independentes.

## 🧾 Padrões do projeto

- Preserve o comportamento existente, a compatibilidade dos schemas e o modo seguro por padrão.
- Use testes determinísticos e credenciais sintéticas sempre que possível.
- Não atualize `docs/CONFORMANCE.md` para declarar um teste aprovado se ele não foi executado.
- Evite dependências novas quando uma solução pequena existente servir.
- Documente comandos e limitações que dependem da distribuição ou do hardware.

## 🏆 Agradecimentos

Agradecemos todas as pessoas que ajudam com código, testes, revisão, documentação e relatos de bugs. Os contribuidores são reconhecidos no [gráfico público do GitHub](https://github.com/sl7187855-coder/harness-rs/graphs/contributors); queremos também agradecer contribuições de teste e documentação nas notas de release quando fizer sentido.

Ao enviar uma contribuição, você concorda que ela será disponibilizada sob a licença Apache-2.0 deste repositório, conforme os termos aplicáveis.
