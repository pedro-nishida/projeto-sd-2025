# projeto-sd-2025 — Eleição de Líder com Heartbeat

Sistema distribuído em Rust com coordenação centralizada dinâmica e detecção de
falhas via heartbeat, conforme Tema 4 da disciplina de Sistemas Distribuídos 2025.

---

## 1. Arquitetura do Projeto

```
             ┌──────────────────────────────────────┐
             │            Cliente HTTP              │
             └─────────────────┬────────────────────┘
                               │ port 80
             ┌─────────────────▼────────────────────┐
             │               Nginx                  │  least_conn load balancing
             └──┬────────────────┬────────────────┬─┘
                │ port 8001      │ port 8002      │ port 8003
        ┌───────▼───────┐  ┌─────▼────────┐  ┌────▼────────┐
        │  node1 (id=1) │  │ node2(id=2)  │  │ node3(id=3) │
        │  Follower     │  │   Follower   │  │    Leader   │
        └────────┬──────┘  └─────┬────────┘  └────┬────────┘
                 │  cluster TCP  │                │
                 └───────────────┴────────────────┘
                    port 9001 / 9002 / 9003
```

### Conceitos e Onde Foram Implementados

| Conceito | Módulo / Arquivo | Descrição |
|---|---|---|
| **Consistência Forte** | `src/sync.rs` | O primário aguarda ACK de todos os backups antes de confirmar a escrita ao cliente. |
| **Balanceamento de Carga** | `nginx.conf` | Método `least_conn` distribui requisições ao nó com menos conexões ativas. |
| **Tolerância a Falhas** | `src/node.rs`, `src/election.rs` | Timeouts TCP na camada de comunicação; pares inacessíveis são ignorados sem travar o cluster. |
| **Eleição de Líder** | `src/election.rs` | Algoritmo Bully: nó com maior ID vence; eleição iniciada por timeout de heartbeat. |
| **Heartbeat** | `src/node.rs` | Líder envia heartbeat a cada 1 s; seguidores disparam eleição após 3 s sem resposta. |
| **Redirecionamento de Carga** | `src/node.rs` | Requisições de escrita recebidas por não-líderes retornam redirect HTTP para o líder atual. |

---

## 2. Estrutura do Projeto

```
projeto-sd-2025/
├── src/
│   ├── main.rs        # Ponto de entrada; parsing de argumentos CLI (clap)
│   ├── node.rs        # Máquina de estados do nó; heartbeat; servidor HTTP (axum)
│   ├── election.rs    # Algoritmo de eleição Bully
│   ├── sync.rs        # Replicação com ACK (consistência forte)
│   └── message.rs     # Tipos de mensagem inter-nó (JSON sobre TCP)
├── nginx.conf         # Configuração Nginx (upstream least_conn)
├── Dockerfile         # Imagem Docker para os nós Rust
├── docker-compose.yml # Cluster de 3 nós + Nginx
└── Cargo.toml
```

---

## 3. Como Executar

### Pré-requisitos
- [Docker](https://docs.docker.com/get-docker/) e Docker Compose
- **ou** Rust 1.70+ para execução local

### Com Docker Compose (recomendado)

```bash
docker compose up --build
```

O cluster iniciará três nós Rust e o Nginx na porta 80.

### Localmente (sem Docker)

```bash
cargo build --release

# Terminal 1 – nó 1
./target/release/node --id 1 --http-port 8001 --cluster-port 9001 \
  --peers 2:127.0.0.1:9002,3:127.0.0.1:9003 \
  --peer-http 2:127.0.0.1:8002,3:127.0.0.1:8003

# Terminal 2 – nó 2
./target/release/node --id 2 --http-port 8002 --cluster-port 9002 \
  --peers 1:127.0.0.1:9001,3:127.0.0.1:9003 \
  --peer-http 1:127.0.0.1:8001,3:127.0.0.1:8003

# Terminal 3 – nó 3 (maior ID → será eleito líder)
./target/release/node --id 3 --http-port 8003 --cluster-port 9003 \
  --peers 1:127.0.0.1:9001,2:127.0.0.1:9002 \
  --peer-http 1:127.0.0.1:8001,2:127.0.0.1:8002
```

---

## 4. API HTTP

| Método | Caminho | Descrição |
|---|---|---|
| `GET` | `/status` | Retorna `id`, `role`, `term` e `leader_id` do nó. |
| `GET` | `/read?key=<k>` | Lê o valor de uma chave do store distribuído. |
| `POST` | `/write?key=<k>&value=<v>` | Escreve um par chave/valor. Não-líderes redirecionam ao líder. |

### Exemplos

```bash
# Ver status de todos os nós via Nginx
curl http://localhost/status

# Escrever um valor (o Nginx roteia ao nó com menos conexões;
# se não for o líder, o nó redireciona automaticamente)
curl -L -X POST "http://localhost/write?key=foo&value=bar"

# Ler de qualquer nó
curl "http://localhost/read?key=foo"
```

---

## 5. Protocolo Inter-Nó

Mensagens trocadas pelos nós via TCP (JSON delimitado por newline):

| Mensagem | Direção | Significado |
|---|---|---|
| `heartbeat` | Líder → Seguidores | Sinal de vivacidade periódico (1 s). |
| `election` | Candidato → Pares com ID maior | Inicia eleição (Bully). |
| `ok` | Par com ID maior → Candidato | "Estou vivo, continuarei a eleição." |
| `coordinator` | Novo líder → Todos | Anuncia o resultado da eleição. |
| `replicate` | Líder → Seguidores | Replica escrita para consistência forte. |
| `replicate_ack` | Seguidor → Líder | Confirma que a escrita foi persistida. |

---

## 6. Testes

```bash
cargo test
```

Cobertura das unidades principais: serialização de mensagens, store
chave/valor, replicação sem pares disponíveis e eleição sem pares superiores.

---

## 7. Cronograma de Implantação (15 dias)

| Etapa | Duração (dias) | Esforço (%) |
|---|---|---|
| Levantamento de Requisitos e Protocolos | 2 | 10% |
| Desenho da Arquitetura e Definição de Conceitos | 2 | 15% |
| Desenvolvimento do Core Distribuído em Rust | 6 | 45% |
| Integração com Nginx e Configuração de Rede | 2 | 10% |
| Testes de Falha, Validação e README | 2 | 10% |
| Preparação de Slides e Apresentação | 1 | 10% |
| **Total** | **15** | **100%** |

---

## 8. Descrição da execução

Para entender o fluxo, é útil pensar na responsabilidade de cada arquivo:

* main.rs: Lê os argumentos do terminal e dá o start nas tarefas simultâneas (threads/tasks).

* node.rs: Controla quem o nó é (Líder ou Seguidor), gerencia o servidor HTTP e o servidor TCP.

* message.rs: Define os formatos exatos (JSON) das mensagens que os nós enviam uns aos outros por TCP.

* election.rs: Contém a lógica pura do algoritmo Bully para decidir quem é eleito como líder.

Parte 1: Boot

Antes de qualquer requisição, o sistema precisa subir:

    1.O main.rs é chamado pelo Docker/Cargo. Ele faz o parsing das portas e dos vizinhos (peers) usando a biblioteca clap.

    2.O main.rs cria o estado compartilhado daquele nó (NodeState::new), que começa sempre como Role::Follower.

    3.O main.rs então usa o Tokio para "dar o play" em três tarefas que vão rodar para sempre e ao mesmo tempo:

        - Chama tokio::spawn(node::cluster_server(...)) para ouvir mensagens TCP de outros nós.

        - Chama tokio::spawn(node::heartbeat_monitor(...)) para ficar vigiando se o líder está vivo.

        - Chama axum::serve(listener, router) para iniciar o servidor HTTP na porta 800X.

Parte 2: O Caminho do curl http://localhost/status

Aqui o caminho é bem direto e focado na leitura do estado:

    1.O comando bate no Nginx (porta 80), que escolhe um nó e manda a requisição para a porta HTTP dele (ex: 8001).

    2.A requisição cai no servidor do main.rs, que repassa imediatamente para as rotas definidas no node.rs dentro da função http_router.

    3.O node.rs vê que a rota é /status e chama a função handle_status.

    4.A handle_status pausa tudo rapidinho (usando um lock), lê as variáveis atuais daquele nó (seu id, se ele é líder/seguidor, etc.) e converte isso para JSON devolvendo para o cliente.

    5.Importante: Note que o message.rs e o election.rs nem são chamados aqui, pois é só uma consulta de leitura simples via HTTP.

Parte 3: Derrubando um Nó (A Eleição Passo a Passo)

Vamos supor que o nó 3 (Líder) foi derrubado. É aqui que os arquivos interagem intensamente via TCP.

    1.O loop infinito do heartbeat_monitor dentro de node.rs dos nós sobreviventes acorda a cada 500ms e checa o relógio. Ele percebe que o líder não manda sinal há mais de 3 segundos (HEARTBEAT_TIMEOUT).

    2.Ainda no heartbeat_monitor, o nó muda seu próprio status de Follower para Candidate e chama a função externa: election::start_election(my_id, &peers).

    3.Dentro de election.rs, o código filtra a lista de vizinhos para achar apenas aqueles com ID maior que o dele. Para cada nó maior, ele chama send_election(addr, my_id).

    4.Dentro de send_election, o código precisa falar com o outro nó. Ele usa o message.rs instanciando Message::Election { candidate_id } e chama to_line(). O message.rs pega essa estrutura Rust e transforma em uma string JSON {"type":"election","candidate_id":2}\n. Essa string é enviada pelo cabo (TCP).

    5.Como o nó 3 está morto, o envio falha (retorna false). O election.rs percebe que ninguém maior respondeu. Ele então chama broadcast_coordinator. Mais uma vez, usa o message.rs para criar a mensagem JSON de vitória (Message::Coordinator { leader_id }) e dispara via TCP para todo mundo que sobrou. A função retorna o próprio ID como vencedor.

    6.A função heartbeat_monitor no node.rs recebe de volta o resultado. Como o ID vencedor é igual ao próprio ID, ele muda seu status para Role::Leader.

    7.Imediatamente, o novo líder aciona um tokio::spawn(heartbeat_sender(...)). Essa nova rotina vai usar o message.rs a cada 1 segundo para criar a string JSON {"type":"heartbeat", ...} e enviar para os seguidores, mantendo a paz no cluster.
