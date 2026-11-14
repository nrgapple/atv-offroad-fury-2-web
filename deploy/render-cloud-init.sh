#!/usr/bin/env bash
# Print deploy/cloud-init.yaml with its placeholders filled: the two public SSH
# keys, and the systemd unit, release helper and env file from this directory.
#
#   deploy/render-cloud-init.sh <admin-key.pub> <deploy-key.pub> [--domain D] [--acme-email E]
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
usage() { echo "usage: $0 <admin-key.pub> <deploy-key.pub> [--domain D] [--acme-email E]" >&2; exit 2; }
[[ $# -ge 2 ]] || usage
admin_key_file=$1 deploy_key_file=$2
shift 2
domain=mp.furyremastered.com email=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --domain) domain=${2:?}; shift 2 ;;
    --acme-email) email=${2:?}; shift 2 ;;
    *) usage ;;
  esac
done
[[ $domain =~ ^[a-z0-9.-]+$ ]] || { echo "invalid domain: $domain" >&2; exit 2; }
[[ -z $email || $email =~ ^[^[:space:]\|\&@]+@[^[:space:]\|\&@]+$ ]] || { echo "invalid email: $email" >&2; exit 2; }

# One public key line: "type base64 [comment]", nothing that could break the YAML.
public_key() {
  local key
  key=$(head -n 1 "$1")
  [[ $key =~ ^(ssh-ed25519|ecdsa-sha2-nistp256|ssh-rsa)\ [A-Za-z0-9+/=]+(\ [A-Za-z0-9@._-]+)?$ ]] \
    || { echo "not a single-line public key: $1" >&2; exit 2; }
  echo "$key"
}
encode() { base64 <"$1" | tr -d '\n'; }

admin_key=$(public_key "$admin_key_file")
deploy_key=$(public_key "$deploy_key_file")
[[ $admin_key != "$deploy_key" ]] || { echo "admin and deploy keys must differ" >&2; exit 2; }

env_file=$(mktemp)
trap 'rm -f "$env_file"' EXIT
sed -e "s|^FURY_ACME_DOMAINS=.*|FURY_ACME_DOMAINS=$domain|" "$here/env.example" >"$env_file"
if [[ -n $email ]]; then
  sed -i.bak -e "s|^# FURY_ACME_EMAIL=.*|FURY_ACME_EMAIL=$email|" "$env_file" && rm -f "$env_file.bak"
fi

out=$(sed \
  -e "s|@ADMIN_SSH_KEY@|$admin_key|" \
  -e "s|@DEPLOY_SSH_KEY@|$deploy_key|" \
  -e "s|@FURY_RELEASE_B64@|$(encode "$here/fury-release")|" \
  -e "s|@FURY_SERVER_UNIT_B64@|$(encode "$here/systemd/fury-server.service")|" \
  -e "s|@FURY_ENV_B64@|$(encode "$env_file")|" \
  -e "s|^fqdn: .*|fqdn: $domain|" \
  "$here/cloud-init.yaml")
if [[ $out =~ @[A-Z0-9_]+@ ]]; then echo "unfilled placeholder: ${BASH_REMATCH[0]}" >&2; exit 1; fi
printf '%s\n' "$out"
