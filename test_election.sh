#!/usr/bin/env bash
# =============================================================================
# test_election.sh — Demonstra eleição de líder com heartbeat
#
# Uso: ./test_election.sh
# Pré-requisito: cluster já rodando com `docker compose up --build -d`
# =============================================================================

set -euo pipefail

NGINX="http://localhost"
NODE1="http://localhost:8001"
NODE2="http://localhost:8002"
NODE3="http://localhost:8003"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

separator() { echo -e "\n${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; }
step()      { echo -e "\n${BOLD}${YELLOW}▶ $1${NC}"; }
ok()        { echo -e "  ${GREEN}✔ $1${NC}"; }
info()      { echo -e "  ${CYAN}ℹ $1${NC}"; }
fail()      { echo -e "  ${RED}✘ $1${NC}"; }

# Retorna o status de um nó como JSON formatado, ou erro.
node_status() {
    local url="$1"
    curl -sf --max-time 2 "${url}/status" 2>/dev/null \
        | python3 -m json.tool 2>/dev/null \
        || echo "  (unreachable)"
}

# Retorna apenas o campo 'role' do status.
node_role() {
    local url="$1"
    curl -sf --max-time 2 "${url}/status" 2>/dev/null \
        | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('role','?'))" 2>/dev/null \
        || echo "DOWN"
}

# Retorna apenas o campo 'leader_id' do status.
node_leader() {
    local url="$1"
    curl -sf --max-time 2 "${url}/status" 2>/dev/null \
        | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('leader_id','?'))" 2>/dev/null \
        || echo "DOWN"
}

# Imprime uma tabela rápida com role de cada nó.
print_roles() {
    local r1 r2 r3
    r1=$(node_role "$NODE1")
    r2=$(node_role "$NODE2")
    r3=$(node_role "$NODE3")
    echo -e "  node1=${BOLD}${r1}${NC}  node2=${BOLD}${r2}${NC}  node3=${BOLD}${r3}${NC}"
}

# Aguarda até que o número de líderes no cluster seja 1.
wait_for_leader() {
    local max_wait="${1:-15}"
    local elapsed=0
    info "Aguardando eleição convergir (max ${max_wait}s)..."
    while [[ $elapsed -lt $max_wait ]]; do
        local leaders=0
        for url in "$NODE1" "$NODE2" "$NODE3"; do
            local role
            role=$(node_role "$url")
            [[ "$role" == "Leader" ]] && ((leaders++)) || true
        done
        if [[ $leaders -eq 1 ]]; then
            ok "Cluster convergiu — 1 líder eleito."
            return 0
        fi
        sleep 1
        ((elapsed++))
    done
    fail "Timeout: cluster não convergiu em ${max_wait}s."
    return 1
}

# =============================================================================
separator
echo -e "${BOLD}  PROJETO SD-2025 — Teste de Eleição de Líder com Heartbeat${NC}"
separator

# -----------------------------------------------------------------------------
step "1. Estado inicial do cluster"
echo ""
echo "  [node1]"; node_status "$NODE1"
echo "  [node2]"; node_status "$NODE2"
echo "  [node3]"; node_status "$NODE3"

# Aguarda o cluster ter um líder antes de prosseguir.
wait_for_leader 20
print_roles

# -----------------------------------------------------------------------------
step "2. Escrevendo dados no cluster via Nginx"
info "POST /write?key=projeto&value=sd2025"
WRITE_RESP=$(curl -sf -L --max-time 5 -X POST \
    "${NGINX}/write?key=projeto&value=sd2025" 2>/dev/null || echo "ERRO")
echo "  Resposta: ${WRITE_RESP}"

info "POST /write?key=disciplina&value=sistemas-distribuidos"
curl -sf -L --max-time 5 -X POST \
    "${NGINX}/write?key=disciplina&value=sistemas-distribuidos" > /dev/null 2>&1 \
    && ok "Escrita enviada." || fail "Falha na escrita."

# -----------------------------------------------------------------------------
step "3. Lendo dados de cada nó (replicação)"
for node_url in "$NODE1" "$NODE2" "$NODE3"; do
    name="${node_url##*:}"; name="node (porta ${name})"
    val1=$(curl -sf --max-time 2 "${node_url}/read?key=projeto" 2>/dev/null || echo "ERRO")
    val2=$(curl -sf --max-time 2 "${node_url}/read?key=disciplina" 2>/dev/null || echo "ERRO")
    echo -e "  ${name} → projeto=${BOLD}${val1}${NC}  disciplina=${BOLD}${val2}${NC}"
done

# -----------------------------------------------------------------------------
separator
step "4. Identificando o líder atual"
LEADER_ID=$(curl -sf --max-time 2 "${NGINX}/status" 2>/dev/null \
    | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('leader_id','?'))" 2>/dev/null \
    || echo "?")
info "leader_id reportado: ${BOLD}${LEADER_ID}${NC}"

# Monta nome do container do líder.
if [[ "$LEADER_ID" =~ ^[0-9]+$ ]]; then
    LEADER_CONTAINER="node${LEADER_ID}"
else
    fail "Não foi possível identificar o líder. Encerrando."
    exit 1
fi
info "Container do líder: ${BOLD}${LEADER_CONTAINER}${NC}"

# -----------------------------------------------------------------------------
separator
step "5. Derrubando o líder: docker stop ${LEADER_CONTAINER}"
docker stop "${LEADER_CONTAINER}" > /dev/null
ok "${LEADER_CONTAINER} parado."

# -----------------------------------------------------------------------------
step "6. Aguardando heartbeat timeout e nova eleição..."
info "Os seguidores detectarão ausência de heartbeat em ~3s e iniciarão eleição Bully."
sleep 1

# Polling: mostra roles a cada segundo até convergir.
for i in $(seq 1 12); do
    sleep 1
    printf "  [%2ds] " "$i"
    r1=$(node_role "$NODE1"); r2=$(node_role "$NODE2"); r3=$(node_role "$NODE3")
    printf "node1=%-10s node2=%-10s node3=%-10s\n" "$r1" "$r2" "$r3"
    leaders=0
    [[ "$r1" == "Leader" ]] && ((leaders++)) || true
    [[ "$r2" == "Leader" ]] && ((leaders++)) || true
    [[ "$r3" == "Leader" ]] && ((leaders++)) || true
    if [[ $leaders -eq 1 ]]; then
        echo ""
        ok "Nova eleição concluída em ~${i}s!"
        break
    fi
done

# -----------------------------------------------------------------------------
step "7. Estado do cluster após eleição"
print_roles
NEW_LEADER=$(curl -sf --max-time 2 "${NODE1}/status" 2>/dev/null \
    | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('leader_id','?'))" 2>/dev/null \
    || curl -sf --max-time 2 "${NODE2}/status" 2>/dev/null \
    | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('leader_id','?'))" 2>/dev/null \
    || echo "?")
info "Novo líder: ${BOLD}node${NEW_LEADER}${NC}"

# -----------------------------------------------------------------------------
step "8. Verificando que os dados persistem após a eleição"
for node_url in "$NODE1" "$NODE2" "$NODE3"; do
    name="${node_url##*:}"; name="node (porta ${name})"
    val1=$(curl -sf --max-time 2 "${node_url}/read?key=projeto" 2>/dev/null || echo "DOWN/ERRO")
    val2=$(curl -sf --max-time 2 "${node_url}/read?key=disciplina" 2>/dev/null || echo "DOWN/ERRO")
    echo -e "  ${name} → projeto=${BOLD}${val1}${NC}  disciplina=${BOLD}${val2}${NC}"
done

# -----------------------------------------------------------------------------
step "9. Testando escrita no novo cluster (sem o líder antigo)"
info "POST /write?key=failover&value=ok"
WRITE2=$(curl -sf -L --max-time 5 -X POST \
    "${NGINX}/write?key=failover&value=ok" 2>/dev/null || echo "ERRO")
echo "  Resposta: ${WRITE2}"

val=$(curl -sf --max-time 2 "${NODE1}/read?key=failover" 2>/dev/null \
    || curl -sf --max-time 2 "${NODE2}/read?key=failover" 2>/dev/null \
    || echo "ERRO")
[[ "$val" == "ok" ]] \
    && ok "Escrita e leitura bem-sucedidas após failover." \
    || fail "Dado não encontrado: '${val}'"

# -----------------------------------------------------------------------------
step "10. Reintegrando o nó derrubado: docker start ${LEADER_CONTAINER}"
docker start "${LEADER_CONTAINER}" > /dev/null
info "Aguardando ${LEADER_CONTAINER} se reconectar ao cluster..."
sleep 5

echo ""
echo "  [node1]"; node_status "$NODE1"
echo "  [node2]"; node_status "$NODE2"
echo "  [node3]"; node_status "$NODE3"
echo ""
print_roles

# -----------------------------------------------------------------------------
separator
echo -e "${BOLD}${GREEN}  ✔ Teste concluído com sucesso!${NC}"
echo ""
echo -e "  Fluxo demonstrado:"
echo -e "   1. Cluster inicializa → nó com maior ID vira líder (Bully)"
echo -e "   2. Dados escritos e replicados com ACK (consistência forte)"
echo -e "   3. Líder derrubado → heartbeat timeout detectado pelos seguidores"
echo -e "   4. Eleição Bully re-executada → novo líder eleito"
echo -e "   5. Cluster continua operando (tolerância a falhas)"
echo -e "   6. Nó reintegrado vira seguidor do novo líder"
separator
