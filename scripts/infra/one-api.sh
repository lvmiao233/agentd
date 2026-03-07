#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

ONE_API_BIN="${ONE_API_BIN:-$(command -v one-api || true)}"
PORT="${ONE_API_PORT:-3000}"
DATA_DIR="${ONE_API_DATA_DIR:-$REPO_ROOT/data/one-api}"
LOG_DIR="${ONE_API_LOG_DIR:-}"
PID_FILE="${ONE_API_PID_FILE:-}"
STDOUT_LOG="${ONE_API_STDOUT_LOG:-}"
HEALTH_URL="http://127.0.0.1:${PORT}/api/status"
STARTUP_TIMEOUT="${ONE_API_STARTUP_TIMEOUT:-30}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()    { echo -e "${BLUE}[one-api]${NC} $1"; }
log_success() { echo -e "${GREEN}[one-api]${NC} $1"; }
log_warn()    { echo -e "${YELLOW}[one-api]${NC} $1"; }
log_error()   { echo -e "${RED}[one-api]${NC} $1" >&2; }

require_binary() {
    if [[ -z "$ONE_API_BIN" ]]; then
        log_error "one-api binary not found; set ONE_API_BIN or install one-api"
        exit 1
    fi
    if [[ ! -x "$ONE_API_BIN" ]]; then
        log_error "one-api binary is not executable: $ONE_API_BIN"
        exit 1
    fi
}

ensure_dirs() {
    mkdir -p "$DATA_DIR" "$LOG_DIR"
}

read_pid_file() {
    if [[ -f "$PID_FILE" ]]; then
        tr -d '[:space:]' < "$PID_FILE"
    fi
}

process_alive() {
    local pid="$1"
    [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

command_matches() {
    local pid="$1"
    local cmdline
    cmdline="$(ps -p "$pid" -o args= 2>/dev/null || true)"
    [[ "$cmdline" == *"one-api"* && "$cmdline" == *"--port ${PORT}"* ]]
}

find_running_pid() {
    local pid
    pid="$(read_pid_file || true)"
    if process_alive "$pid" && command_matches "$pid"; then
        printf '%s\n' "$pid"
        return 0
    fi

    pid="$(pgrep -f "(^|/)one-api( |$).*--port ${PORT}( |$)" | head -n 1 || true)"
    if process_alive "$pid"; then
        printf '%s\n' "$pid"
        return 0
    fi

    return 1
}

write_pid_file() {
    local pid="$1"
    printf '%s\n' "$pid" > "$PID_FILE"
}

clear_pid_file() {
    rm -f "$PID_FILE"
}

latest_log_file() {
    local latest
    latest="$(ls -1t "$LOG_DIR" 2>/dev/null | head -n 1 || true)"
    if [[ -n "$latest" ]]; then
        printf '%s\n' "$LOG_DIR/$latest"
    else
        printf '%s\n' "$STDOUT_LOG"
    fi
}

wait_healthy() {
    local elapsed=0
    log_info "waiting for health at $HEALTH_URL (timeout: ${STARTUP_TIMEOUT}s) ..."
    while [[ "$elapsed" -lt "$STARTUP_TIMEOUT" ]]; do
        if curl --noproxy '*' -sf "$HEALTH_URL" >/dev/null 2>&1; then
            log_success "healthy after ${elapsed}s"
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done

    log_error "health check timed out after ${STARTUP_TIMEOUT}s"
    log_warn "recent logs:"
    tail -n 20 "$(latest_log_file)" 2>/dev/null || true
    return 1
}

do_start() {
    require_binary
    ensure_dirs

    local pid
    pid="$(find_running_pid || true)"
    if process_alive "$pid"; then
        write_pid_file "$pid"
        log_warn "one-api already running on port ${PORT} (pid=${pid})"
        do_status
        return 0
    fi

    log_info "starting one-api binary: bin=$ONE_API_BIN port=$PORT data=$DATA_DIR logs=$LOG_DIR"
    (
        cd "$DATA_DIR"
        nohup "$ONE_API_BIN" --port "$PORT" --log-dir "$LOG_DIR" >> "$STDOUT_LOG" 2>&1 &
        printf '%s\n' "$!" > "$PID_FILE"
    )

    pid="$(read_pid_file || true)"
    if ! process_alive "$pid"; then
        log_error "one-api process exited immediately"
        tail -n 20 "$STDOUT_LOG" 2>/dev/null || true
        return 1
    fi

    wait_healthy
    echo ""
    log_success "One API running at http://127.0.0.1:${PORT}"
    log_info "binary: $ONE_API_BIN"
    log_info "data dir: $DATA_DIR"
    log_info "log dir: $LOG_DIR"
}

do_stop() {
    local pid
    pid="$(find_running_pid || true)"
    if ! process_alive "$pid"; then
        clear_pid_file
        log_warn "one-api is not running on port ${PORT}"
        return 0
    fi

    log_info "stopping one-api (pid=${pid}) ..."
    kill "$pid"
    local attempts=0
    while process_alive "$pid" && [[ "$attempts" -lt 10 ]]; do
        sleep 1
        attempts=$((attempts + 1))
    done

    if process_alive "$pid"; then
        log_warn "process did not exit cleanly; sending SIGKILL"
        kill -9 "$pid"
    fi

    clear_pid_file
    log_success "stopped"
}

do_restart() {
    do_stop
    sleep 1
    do_start
}

do_status() {
    local pid
    pid="$(find_running_pid || true)"
    if process_alive "$pid"; then
        write_pid_file "$pid"
        log_success "one-api is running on port ${PORT}"
        ps -p "$pid" -o pid=,etime=,args=
        echo ""
        if curl --noproxy '*' -sf "$HEALTH_URL" >/dev/null 2>&1; then
            log_success "health endpoint OK"
        else
            log_warn "health endpoint unreachable"
        fi
    else
        clear_pid_file
        log_info "one-api is not running on port ${PORT}"
    fi
}

do_logs() {
    ensure_dirs
    local follow_flag=""
    if [[ "${1:-}" == "--follow" || "${1:-}" == "-f" ]]; then
        follow_flag="-f"
    fi
    local log_file
    log_file="$(latest_log_file)"
    if [[ ! -f "$log_file" ]]; then
        log_warn "no log file found under $LOG_DIR"
        return 0
    fi
    tail $follow_flag -n 100 "$log_file"
}

do_reset() {
    log_warn "this will stop one-api on port ${PORT} and DELETE all data at $DATA_DIR"
    do_stop
    if [[ -d "$DATA_DIR" ]]; then
        rm -rf "$DATA_DIR"
        log_info "data directory removed: $DATA_DIR"
    fi
    log_success "reset complete"
}

do_health() {
    local token="${ONE_API_TOKEN:-}"

    echo "=== One API Health Check ==="
    echo ""

    if ! curl --noproxy '*' -sf "$HEALTH_URL" >/dev/null 2>&1; then
        log_error "FAIL: $HEALTH_URL unreachable"
        echo "HEALTH=false"
        return 1
    fi
    log_success "PASS: health endpoint OK"

    local status_code
    status_code="$(curl --noproxy '*' -s -o /dev/null -w "%{http_code}" "$HEALTH_URL" 2>/dev/null || echo "000")"
    if [[ "$status_code" == "200" ]]; then
        log_success "PASS: /api/status -> $status_code"
    else
        log_warn "WARN: /api/status -> $status_code"
    fi

    if [[ -z "$token" ]]; then
        log_warn "SKIP: model check (ONE_API_TOKEN not set)"
        echo "HEALTH=true"
        echo "MODELS_CHECKED=false"
        return 0
    fi

    local models_url="http://127.0.0.1:${PORT}/v1/models"
    local models_response
    models_response="$(curl --noproxy '*' -sf -H "Authorization: Bearer $token" "$models_url" 2>/dev/null || echo "")"
    if [[ -n "$models_response" ]]; then
        local model_count
        model_count="$(python3 -c "import json,sys; print(len(json.load(sys.stdin).get('data', [])))" <<< "$models_response" 2>/dev/null || echo "0")"
        log_success "PASS: /v1/models -> $model_count model(s) available"
        python3 -c 'import json,sys
for model in json.load(sys.stdin).get("data", []):
    print(f"  - {model.get("id", "?")}")' <<< "$models_response" 2>/dev/null || true
    else
        log_warn "WARN: /v1/models returned empty (token may be invalid or no channels configured)"
    fi

    echo ""
    echo "HEALTH=true"
    echo "MODELS_CHECKED=true"
}

usage() {
    cat <<'EOF'
Usage: scripts/infra/one-api.sh <command> [options]

Commands:
  start     Start one-api binary in the background
  stop      Stop the running one-api process for the configured port
  restart   Stop then start
  status    Show process + health status
  logs      Show one-api logs (--follow for tail -f)
  reset     Stop process and delete all one-api data (destructive!)
  health    Run readiness checks (health + optional models)

Environment variables:
  ONE_API_BIN           Binary path (default: resolved from PATH)
  ONE_API_PORT          Listen port (default: 3000)
  ONE_API_DATA_DIR      Data directory / working directory (default: <repo>/data/one-api)
  ONE_API_LOG_DIR       Log directory (default: <data-dir>/logs)
  ONE_API_PID_FILE      PID file path (default: <data-dir>/one-api.pid)
  ONE_API_STDOUT_LOG    Stdout/stderr log file (default: <log-dir>/one-api-stdout.log)
  ONE_API_TOKEN         API token for model checks in 'health' command

Examples:
  scripts/infra/one-api.sh start
  scripts/infra/one-api.sh start --port 3011 --data-dir /tmp/one-api-dev
  scripts/infra/one-api.sh status
  ONE_API_TOKEN=sk-xxx scripts/infra/one-api.sh health
  scripts/infra/one-api.sh logs --follow
  scripts/infra/one-api.sh reset --data-dir /tmp/one-api-dev
EOF
}

parse_common_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --port)
                PORT="$2"
                shift 2
                ;;
            --data-dir)
                DATA_DIR="$2"
                shift 2
                ;;
            --log-dir)
                LOG_DIR="$2"
                shift 2
                ;;
            --bin)
                ONE_API_BIN="$2"
                shift 2
                ;;
            --follow|-f)
                EXTRA_ARG="--follow"
                shift
                ;;
            *)
                log_error "unknown option: $1"
                exit 1
                ;;
        esac
    done

    if [[ -z "$LOG_DIR" ]]; then
        LOG_DIR="$DATA_DIR/logs"
    fi
    if [[ -z "$PID_FILE" ]]; then
        PID_FILE="$DATA_DIR/one-api.pid"
    fi
    if [[ -z "$STDOUT_LOG" ]]; then
        STDOUT_LOG="$LOG_DIR/one-api-stdout.log"
    fi
    HEALTH_URL="http://127.0.0.1:${PORT}/api/status"
}

if [[ $# -lt 1 ]]; then
    usage
    exit 1
fi

COMMAND="$1"
shift
EXTRA_ARG=""
parse_common_args "$@"

case "$COMMAND" in
    start)   do_start ;;
    stop)    do_stop ;;
    restart) do_restart ;;
    status)  do_status ;;
    logs)    do_logs "$EXTRA_ARG" ;;
    reset)   do_reset ;;
    health)  do_health ;;
    -h|--help|help)
        usage
        ;;
    *)
        log_error "unknown command: $COMMAND"
        usage
        exit 1
        ;;
esac
