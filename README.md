# projeto-sd-2025 — Eleição de Líder com Heartbeat

Sistema distribuído em Rust com coordenação centralizada dinâmica e detecção de
falhas via heartbeat, conforme Tema 4 da disciplina de Sistemas Distribuídos 2025.

---

## 1. Arquitetura do Projeto

```
          ┌─────────────────────────────────────────┐
          │              Cliente HTTP                │
          └───────────────────┬─────────────────────┘
                              │ port 80
                    ┌─────────▼─────────┐
                    │       Nginx        │  least_conn load balancing
                    └──┬─────┬─────┬────┘
             port 8001 │     │8002 │ 8003
          ┌────────────▼┐  ┌─▼────────┐  ┌─▼────────┐
          │  node1 (id=1│  │ node2(id=2│  │ node3(id=3│
          │  Follower   │  │ Follower  │  │  Leader  │
          └──────┬──────┘  └─────┬────┘  └────┬─────┘
                 │  cluster TCP  │             │
                 └───────────────┴─────────────┘
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
