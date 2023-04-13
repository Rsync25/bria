REPO_ROOT=$(git rev-parse --show-toplevel)
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-${REPO_ROOT##*/}}"
BRIA_HOME="${BRIA_HOME:-.bria}"
if [[ "${BRIA_CONFIG}" == "docker" ]]; then
  COMPOSE_FILE_ARG="-f docker-compose.yml"
fi

bria_cmd() {
  bria_location=${REPO_ROOT}/target/debug/bria
  if [[ ! -z ${CARGO_TARGET_DIR} ]] ; then
    bria_location=${CARGO_TARGET_DIR}/debug/bria
  fi

  ${bria_location} $@
}

cache_default_wallet_balance() {
  balance=$(bria_cmd wallet-balance -w default)
}

cached_pending_income() {
  echo ${balance} | jq -r '.pendingIncomingUtxos'
}

cached_encumbered_fees() {
  echo ${balance} | jq -r '.encumberedFees'
}

cached_current_settled() {
  echo ${balance} | jq -r '.confirmedUtxos'
}

cached_logical_settled() {
  echo ${balance} | jq -r '.logicalSettled'
}

cached_pending_outgoing() {
  echo ${balance} | jq -r '.logicalPendingOutgoing'
}

cached_pending_fees() {
  echo ${balance} | jq -r '.pendingFees'
}

cached_encumbered_outgoing() {
  echo ${balance} | jq -r '.logicalEncumberedOutgoing'
}

bitcoin_cli() {
  docker exec "${COMPOSE_PROJECT_NAME}-bitcoind-1" bitcoin-cli $@
}

lnd_cli() {
  docker exec "${COMPOSE_PROJECT_NAME}-lnd-1" lncli -n regtest $@
}

reset_pg() {
  docker exec "${COMPOSE_PROJECT_NAME}-postgres-1" psql $PG_CON -c "DROP SCHEMA public CASCADE"
  docker exec "${COMPOSE_PROJECT_NAME}-postgres-1" psql $PG_CON -c "CREATE SCHEMA public"
}

restart_bitcoin() {
  docker compose ${COMPOSE_FILE_ARG} rm -sfv bitcoind lnd fulcrum || true
  # Running this twice has sometimes bitcoind is dangling in CI
  docker compose ${COMPOSE_FILE_ARG} rm -sfv bitcoind lnd fulcrum || true
  docker compose ${COMPOSE_FILE_ARG} up -d bitcoind lnd fulcrum
  retry 10 1 lnd_cli getinfo
}

bitcoind_init() {
  bitcoin_cli createwallet "default" || true
	bitcoin_cli -generate 200
}

start_daemon() {
  background bria_cmd daemon --config ./tests/e2e/bria.${BRIA_CONFIG:-local}.yml > .e2e-logs
  sleep 5 # wait for daemon to be up and running
}

stop_daemon() {
  if [[ -f ${BRIA_HOME}/daemon_pid ]]; then
    kill -9 $(cat ${BRIA_HOME}/daemon_pid) || true
  fi
}

bria_init() {
  bria_cmd admin bootstrap
  bria_cmd admin create-account -n default
  sleep 3
  bria_cmd import-xpub -x tpubDD4vFnWuTMEcZiaaZPgvzeGyMzWe6qHW8gALk5Md9kutDvtdDjYFwzauEFFRHgov8pAwup5jX88j5YFyiACsPf3pqn5hBjvuTLRAseaJ6b4 -n key1 -d m/84h/0h/0h
	bria_cmd create-wallet -n default -x key1

  echo "Bria Initialization Complete"
}

# Run the given command in the background. Useful for starting a
# node and then moving on with commands that exercise it for the
# test.
#
# Ensures that BATS' handling of file handles is taken into account;
# see
# https://github.com/bats-core/bats-core#printing-to-the-terminal
# https://github.com/sstephenson/bats/issues/80#issuecomment-174101686
# for details.
background() {
  "$@" 3>- &
  echo $!
}

# Taken from https://github.com/docker/swarm/blob/master/test/integration/helpers.bash
# Retry a command $1 times until it succeeds. Wait $2 seconds between retries.
retry() {
  local attempts=$1
  shift
  local delay=$1
  shift
  local i

  for ((i=0; i < attempts; i++)); do
    run "$@"
    if [[ "$status" -eq 0 ]] ; then
      return 0
    fi
    sleep "$delay"
  done

  echo "Command \"$*\" failed $attempts times. Output: $output"
  false
}
