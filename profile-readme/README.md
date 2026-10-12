<div align="center">

# 👋 Opa! Eu sou Samuel

### Estou construindo o **Thomas — um assistente-agente de IA open-source**

**Não é só um harness.** É um projeto integrado que reúne um agente em desenvolvimento, o runtime Rust Harness-RS, uma HUD web e o Thomas Mobile Agent para Android. O objetivo é evoluir de conversas para tarefas com ferramentas, contexto persistente e autonomia com limites de segurança.

[![Harness-RS CI](https://github.com/sl7187855-coder/harness-rs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/sl7187855-coder/harness-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/sl7187855-coder/harness-rs/blob/main/LICENSE)
[![Help wanted](https://img.shields.io/badge/contributors-needed-brightgreen.svg)](https://github.com/sl7187855-coder/harness-rs/issues/1)

**[🚀 Projeto completo](https://github.com/sl7187855-coder/harness-rs) · [🖥️ HUD Web](https://github.com/sl7187855-coder/harness-rs/blob/main/web/thomas-hud.html) · [📱 Agente Android](https://github.com/sl7187855-coder/harness-rs/releases/tag/mobile-agent-v0.5.3) · [🤝 Contribuir](https://github.com/sl7187855-coder/harness-rs/issues/1)**

<img src="https://raw.githubusercontent.com/sl7187855-coder/harness-rs/main/assets/thomas-system-map.svg" alt="Diagrama do sistema Thomas: HUD Web, Harness-RS, agente Android, modelo, ferramentas, contexto, memória, aprovações e auditoria." width="100%"/>

<sub>Early alpha · visão integrada, com limitações e tarefas de validação documentadas publicamente.</sub>

</div>

---

## 🧩 As partes em conjunto

| Parte | Para que serve |
|---|---|
| **Thomas — assistente/agente** | A visão do produto: planejar, usar ferramentas, acompanhar e verificar tarefas. |
| **Harness-RS (Rust)** | Runtime, loop do agente, gateway, políticas e execução de ferramentas. |
| **HUD Web** | Interface para conversar com o runtime e acompanhar eventos. |
| **Contexto, memória e tarefas** | Estado persistente e continuidade de trabalho, com recursos avançados ainda em evolução. |
| **Thomas Mobile Agent** | App Android e componentes de integração móvel; teste físico ainda precisa ser revalidado. |

## 🧠 A visão do Thomas

Quero ajudar a construir um assistente-agente capaz de **planejar e executar tarefas longas, usar ferramentas, guardar contexto, aprender com resultados e continuar o trabalho com mais consistência**. A meta inclui memória persistente, skills, integrações e autonomia com aprovações e segurança — algumas dessas capacidades ainda estão em desenvolvimento.

## 🥔 Construído num PC batata

Este projeto é desenvolvido numa máquina Positivo antiga: **Intel Atom N2600 a 1,60 GHz, 2 núcleos/4 threads e aproximadamente 2 GiB de RAM**. O sistema reporta uma data de firmware/BIOS de `04/11/2014` (isso não confirma a data exata de fabricação do equipamento).

Os builds são lentos e não conseguimos fazer sozinhos toda a bateria de testes longos, integrações e verificações em dispositivos. Por isso, **vou continuar lançando atualizações, mas preciso de contribuidores** para levar o Thomas mais longe.

## 🛠️ No que estou trabalhando

- 🦀 Runtime de agentes, ferramentas e streaming de provedores em Rust
- 🧠 Contexto persistente, memória, tarefas e workflows de longa duração
- 🔐 Aprovações, sandbox Linux, auditoria e higiene de segredos
- 🧩 Skills e integrações MCP/LSP; evolução do suporte a plugins
- 📱 Thomas Mobile Agent, relay e integração com Android
- ⚡ Redução de uso de memória, testes reproduzíveis e suporte a computadores modestos

## 🤝 O projeto precisa de você

Não precisa chegar sabendo tudo. Você pode ajudar executando testes, tentando compilar em outra distribuição Linux, revisando segurança, melhorando a documentação ou pegando uma tarefa fácil marcada no GitHub.

- 🌱 [Issues para iniciantes (`good first issue`)](https://github.com/sl7187855-coder/harness-rs/labels/good%20first%20issue)
- 🧑‍💻 [Guia de contribuição](https://github.com/sl7187855-coder/harness-rs/blob/main/CONTRIBUTING.md)
- 🐛 [Reportar bug ou sugerir melhoria](https://github.com/sl7187855-coder/harness-rs/issues/new/choose)
- 📊 [Ver CI e testes](https://github.com/sl7187855-coder/harness-rs/actions)

### 🏆 Créditos à comunidade

Toda correção, revisão, teste e melhoria de documentação merece reconhecimento. A lista pública cresce com as contribuições registradas no GitHub:

[![Contributors](https://contrib.rocks/image?repo=sl7187855-coder/harness-rs)](https://github.com/sl7187855-coder/harness-rs/graphs/contributors)

[Ver todas as pessoas que contribuíram](https://github.com/sl7187855-coder/harness-rs/graphs/contributors)

---

<div align="center">

**Uma ideia ambiciosa. Um PC batata. Uma comunidade pode fazer a diferença.** ❤️

[⭐ Harness-RS](https://github.com/sl7187855-coder/harness-rs) · [🤝 Ajude a construir](https://github.com/sl7187855-coder/harness-rs/issues/1)

</div>
