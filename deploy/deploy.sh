#!/usr/bin/env bash
# Deploy a fury-server binary to a host prepared by deploy/cloud-init.yaml.
#
#   deploy/deploy.sh <host> <binary> <commit>
#
# Uploads to /opt/fury-server/releases/<commit> (checksum and --version checked
# on the host), switches `current`, restarts, and polls https://<host>/healthz
# until it reports <commit>. If it does not within FURY_HEALTH_TIMEOUT seconds,
# it prints the service log, rolls back to the previous release and fails.
# Keeps the newest FURY_KEEP releases. Running it again for a live commit is a
# no-op. Works from macOS and the GitHub runner (bash, ssh, curl).
#
# Environment (all optional):
#   FURY_SSH_KEY          identity file for the deploy user
#   FURY_KNOWN_HOSTS      known_hosts file; when set, host keys must match it
#   FURY_SSH_USER         default deploy
#   FURY_HEALTH_URL       default https://<host>/healthz
#   FURY_HEALTH_TIMEOUT   seconds, default 90 (a first certificate takes a few)
#   FURY_KEEP             releases kept, default 5
set -euo pipefail

usage() { echo "usage: $0 <host> <binary> <commit>" >&2; exit 2; }
[[ $# -eq 3 ]] || usage
host=$1 binary=$2 commit=$3
[[ $host =~ ^[A-Za-z0-9.:-]+$ ]] || { echo "invalid host: $host" >&2; exit 2; }
[[ $commit =~ ^[0-9a-f]{7,40}$ ]] || { echo "commit must be a lowercase hex SHA: $commit" >&2; exit 2; }
[[ -f $binary ]] || { echo "binary not found: $binary" >&2; exit 2; }

user=${FURY_SSH_USER:-deploy}
health_url=${FURY_HEALTH_URL:-https://$host/healthz}
timeout=${FURY_HEALTH_TIMEOUT:-90}
keep=${FURY_KEEP:-5}

ssh_options=(-o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=15)
if [[ -n ${FURY_SSH_KEY:-} ]]; then ssh_options+=(-i "$FURY_SSH_KEY" -o IdentitiesOnly=yes); fi
if [[ -n ${FURY_KNOWN_HOSTS:-} ]]; then
  ssh_options+=(-o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$FURY_KNOWN_HOSTS")
fi

# Every remote call is one fury-release subcommand (the deploy key's forced command).
# shellcheck disable=SC2029 # arguments are validated commits and numbers
remote() { ssh "${ssh_options[@]}" "$user@$host" "$@"; }

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# The commit /healthz reports when it answers {"ok":true,...}; empty otherwise.
live_commit() {
  local body
  body=$(curl --silent --show-error --fail --max-time 5 "$health_url" 2>/dev/null) || return 0
  [[ $body =~ \"ok\":[[:space:]]*true ]] || return 0
  if [[ $body =~ \"commit\":[[:space:]]*\"([^\"]*)\" ]]; then echo "${BASH_REMATCH[1]}"; fi
}

# Wait until /healthz reports $1.
wait_for() {
  local want=$1 deadline=$((SECONDS + timeout)) seen=""
  while ((SECONDS < deadline)); do
    seen=$(live_commit)
    if [[ $seen == "$want" ]]; then return 0; fi
    sleep 2
  done
  echo "health check: wanted $want, last saw ${seen:-no healthy answer}" >&2
  return 1
}

if [[ $(live_commit) == "$commit" ]]; then
  echo "$commit is already live on $host"
  exit 0
fi

previous=$(remote current)
echo "deploying $commit to $host (live release: ${previous:-none})"
checksum=$(sha256 "$binary")
remote upload "$commit" "$checksum" <"$binary"

if remote activate "$commit" && wait_for "$commit"; then
  remote prune "$keep"
  echo "$commit is live on $host"
  exit 0
fi

echo "deploy of $commit failed; service log:" >&2
remote logs >&2 || true
if [[ -z $previous || $previous == "$commit" ]]; then
  echo "no previous release to roll back to" >&2
  exit 1
fi
remote rollback
if wait_for "$previous"; then
  echo "$previous is healthy again after the rollback" >&2
else
  echo "rollback to $previous is not healthy either" >&2
fi
exit 1
