#!/usr/bin/env bash
# Create (or check) the fury-server droplet on DigitalOcean with doctl.
#
#   deploy/digitalocean/provision.sh [--replace] [options]
#
# Idempotent: SSH keys, tag, Cloud Firewall, droplet, Reserved IP and project
# membership are created only when missing; the firewall rules are reapplied
# every run. --replace builds a fresh droplet from deploy/cloud-init.yaml,
# waits for cloud-init, moves the Reserved IP to it and deletes the old one, so
# DNS never changes. doctl authenticates with DIGITALOCEAN_ACCESS_TOKEN or its
# active context. See deploy/digitalocean/README.md.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
name=${FURY_DO_NAME:-fury-game}
# s-1vcpu-1gb is not offered in nyc1/nyc3 (2026-11-13); nyc2 is the same metro.
region=${FURY_DO_REGION:-nyc2}
size=${FURY_DO_SIZE:-s-1vcpu-1gb}
image=${FURY_DO_IMAGE:-ubuntu-24-04-x64}
project=${FURY_DO_PROJECT:-fury-game}
tag=${FURY_DO_TAG:-fury-game}
firewall=${FURY_DO_FIREWALL:-fury-game}
admin_key=${FURY_ADMIN_KEY:-$HOME/.ssh/fury_game_admin.pub}
deploy_key=${FURY_DEPLOY_KEY:-$HOME/.ssh/fury_game_deploy.pub}
domain=mp.furyremastered.com
acme_email=""
replace=false

usage() {
  cat >&2 <<EOF
usage: $0 [--replace] [--name N] [--region R] [--size S] [--project P]
          [--admin-key FILE.pub] [--deploy-key FILE.pub] [--domain D] [--acme-email E]
defaults: --name $name --region $region --size $size --project $project
EOF
  exit 2
}
while [[ $# -gt 0 ]]; do
  case $1 in
    --replace) replace=true; shift ;;
    --name) name=${2:?}; shift 2 ;;
    --region) region=${2:?}; shift 2 ;;
    --size) size=${2:?}; shift 2 ;;
    --project) project=${2:?}; shift 2 ;;
    --admin-key) admin_key=${2:?}; shift 2 ;;
    --deploy-key) deploy_key=${2:?}; shift 2 ;;
    --domain) domain=${2:?}; shift 2 ;;
    --acme-email) acme_email=${2:?}; shift 2 ;;
    -h | --help) usage ;;
    *) echo "unknown option: $1" >&2; usage ;;
  esac
done

step() { printf '\n== %s\n' "$*"; }
die() { echo "provision: $*" >&2; exit 1; }

command -v doctl >/dev/null || die "doctl is not installed (brew install doctl)"
for key in "$admin_key" "$deploy_key"; do [[ -f $key ]] || die "missing public key $key"; done
admin_identity=${admin_key%.pub}
[[ -f $admin_identity ]] || die "missing private admin key $admin_identity (used to watch cloud-init)"
doctl account get --format Status --no-header >/dev/null || die "doctl is not authenticated"

# Field $2 of the row whose field $1 equals $3, from doctl's text output.
lookup() { awk -v want="$2" -v col="$1" -v out="$3" '$col == want { print $out }'; }

step "Size $size in $region"
regions=$(doctl compute size list --output json | python3 -c '
import json, sys
size = {s["slug"]: s for s in json.load(sys.stdin)}.get(sys.argv[1])
print(" ".join(sorted(size["regions"])) if size and size["available"] else "")' "$size")
[[ " $regions " == *" $region "* ]] || die "$size is not offered in $region; it is in: ${regions:-no region}"
echo "available (also in: $regions)"

step "Project $project"
project_id=$(doctl projects list --format ID,Name --no-header | lookup 2 "$project" 1 | head -n 1)
[[ -n $project_id ]] || die "project $project does not exist (create it in the control panel)"
echo "$project_id"

step "SSH keys"
key_id() {
  local file=$1 label=$2 fingerprint id
  fingerprint=$(ssh-keygen -E md5 -lf "$file" | awk '{ print $2 }' | sed 's/^MD5://')
  id=$(doctl compute ssh-key list --format ID,FingerPrint --no-header | lookup 2 "$fingerprint" 1 | head -n 1)
  if [[ -z $id ]]; then
    id=$(doctl compute ssh-key import "$label" --public-key-file "$file" --format ID --no-header)
    echo "imported $label ($fingerprint) as $id" >&2
  else
    echo "$label present ($fingerprint) as $id" >&2
  fi
  echo "$id"
}
admin_key_id=$(key_id "$admin_key" fury-game-admin)
# Uploaded for the record only: the deploy key is never placed on root. cloud-init
# gives it to the deploy user, restricted to fury-release.
key_id "$deploy_key" fury-game-deploy >/dev/null

step "Tag $tag"
doctl compute tag get "$tag" --format Name --no-header >/dev/null 2>&1 || doctl compute tag create "$tag" >/dev/null

step "Cloud Firewall $firewall (applies to tag $tag)"
anywhere="address:0.0.0.0/0,address:::/0"
inbound="protocol:tcp,ports:22,$anywhere protocol:tcp,ports:443,$anywhere protocol:udp,ports:443,$anywhere protocol:icmp,$anywhere"
outbound="protocol:tcp,ports:all,$anywhere protocol:udp,ports:all,$anywhere protocol:icmp,$anywhere"
firewall_id=$(doctl compute firewall list --format ID,Name --no-header | lookup 2 "$firewall" 1 | head -n 1)
if [[ -z $firewall_id ]]; then
  firewall_id=$(doctl compute firewall create --name "$firewall" --tag-names "$tag" \
    --inbound-rules "$inbound" --outbound-rules "$outbound" --format ID --no-header)
  echo "created $firewall_id"
else
  doctl compute firewall update "$firewall_id" --name "$firewall" --tag-names "$tag" \
    --inbound-rules "$inbound" --outbound-rules "$outbound" --format ID --no-header >/dev/null
  echo "updated $firewall_id"
fi

step "Droplet $name"
existing=$(doctl compute droplet list --tag-name "$tag" --format ID,Name --no-header | lookup 2 "$name" 1)
[[ $(wc -w <<<"$existing") -le 1 ]] || die "more than one $name droplet ($existing); delete the stale one first"
droplet_id=$existing
if [[ -z $droplet_id || $replace == true ]]; then
  user_data=$(mktemp)
  trap 'rm -f "$user_data"' EXIT
  render=("$here/../render-cloud-init.sh" "$admin_key" "$deploy_key" --domain "$domain")
  if [[ -n $acme_email ]]; then render+=(--acme-email "$acme_email"); fi
  "${render[@]}" >"$user_data"
  droplet_id=$(doctl compute droplet create "$name" --size "$size" --image "$image" --region "$region" \
    --ssh-keys "$admin_key_id" --user-data-file "$user_data" --tag-names "$tag" \
    --enable-monitoring --enable-ipv6 --project-id "$project_id" --wait --format ID --no-header)
  echo "created droplet $droplet_id ($size, $region)"
else
  echo "droplet $droplet_id exists (use --replace to rebuild it from cloud-init)"
fi
doctl projects resources assign "$project_id" --resource "do:droplet:$droplet_id" >/dev/null
droplet_ip=$(doctl compute droplet get "$droplet_id" --format PublicIPv4 --no-header)
droplet_ipv6=$(doctl compute droplet get "$droplet_id" --format PublicIPv6 --no-header)

step "cloud-init on $droplet_ip"
known_hosts=$(mktemp)
trap 'rm -f "${user_data:-}" "$known_hosts"' EXIT
admin_ssh=(ssh -i "$admin_identity" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=10
  -o StrictHostKeyChecking=accept-new -o "UserKnownHostsFile=$known_hosts" "ops@$droplet_ip")
deadline=$((SECONDS + 900))
until "${admin_ssh[@]}" true 2>/dev/null; do
  ((SECONDS < deadline)) || die "ops SSH to $droplet_ip did not come up within 15 minutes (cloud-init may have failed to create users; see deploy/digitalocean/README.md)"
  sleep 10
done
if ! "${admin_ssh[@]}" cloud-init status --wait --long; then
  "${admin_ssh[@]}" sudo tail -n 40 /var/log/cloud-init-output.log >&2 || true
  die "cloud-init did not finish cleanly on $droplet_ip"
fi
"${admin_ssh[@]}" 'set -e
  test -x /usr/local/bin/fury-release
  systemctl is-enabled fury-server.service
  sudo sshd -T | grep -E "^(permitrootlogin|passwordauthentication|allowusers) "
  sudo stat -c "%U:%G %a %n" /opt/fury-server/releases /var/lib/fury-server /etc/fury-server /etc/fury-server/env'
host_key=$("${admin_ssh[@]}" cat /etc/ssh/ssh_host_ed25519_key.pub | awk '{ print $1, $2 }')

step "Reserved IP"
# Ours is the one on our (old or new) droplet, account wide; otherwise an
# unassigned one in this project and region (left by an interrupted run).
project_ips=" $(doctl projects resources list "$project_id" --format URN --no-header | sed -n -E 's/^do:(floatingip|reservedip)://p' | tr '\n' ' ') "
reserved="" unassigned=""
while read -r ip ip_region ip_droplet; do
  if [[ -n ${ip_droplet:-} && ($ip_droplet == "$existing" || $ip_droplet == "$droplet_id") ]]; then reserved=$ip; fi
  if [[ -z ${ip_droplet:-} && $ip_region == "$region" && $project_ips == *" $ip "* ]]; then unassigned=$ip; fi
done < <(doctl compute reserved-ip list --format IP,Region,DropletID --no-header)
reserved=${reserved:-$unassigned}

on_droplet() { [[ $(doctl compute reserved-ip get "$reserved" --format DropletID --no-header) == "$droplet_id" ]]; }
wait_on_droplet() {
  local deadline=$((SECONDS + 180))
  until on_droplet; do
    ((SECONDS < deadline)) || die "Reserved IP $reserved did not reach droplet $droplet_id"
    sleep 5
  done
}
if [[ -z $reserved ]]; then
  # Created with --droplet-id, it is assigned asynchronously.
  reserved=$(doctl compute reserved-ip create --droplet-id "$droplet_id" --format IP --no-header)
  wait_on_droplet
  echo "created $reserved on $droplet_id"
elif ! on_droplet; then
  doctl compute reserved-ip-action assign "$reserved" "$droplet_id" --format ID --no-header >/dev/null
  wait_on_droplet
  echo "assigned $reserved to $droplet_id"
else
  echo "$reserved already on $droplet_id"
fi
doctl projects resources assign "$project_id" --resource "do:reservedip:$reserved" >/dev/null
# DigitalOcean accepts this but does not list firewalls under projects; the
# firewall applies to the droplet by tag either way.
doctl projects resources assign "$project_id" --resource "do:firewall:$firewall_id" >/dev/null 2>&1 || true

if [[ $replace == true && -n $existing && $existing != "$droplet_id" ]]; then
  step "Deleting the replaced droplet $existing"
  doctl compute droplet delete "$existing" --force
fi

step "Done"
cat <<EOF
Droplet      $name ($droplet_id), $size in $region
Public IPs   $droplet_ip, $droplet_ipv6 (change on --replace)
Reserved IP  $reserved (stable; point DNS here)

Next steps:
 1. DNS (Cloudflare, DNS only / grey cloud): A  ${domain%%.*}  ->  $reserved
 2. GitHub secrets for .github/workflows/game-server.yml (repeat 2b after every --replace):
    a. gh secret set GAME_SERVER_HOST --body $domain
    b. gh secret set GAME_SERVER_KNOWN_HOSTS --body '$domain,$reserved $host_key'
    c. gh secret set GAME_SERVER_SSH_KEY < ${deploy_key%.pub}
 3. Deploy: gh workflow run game-server.yml --ref main
    (or deploy/deploy.sh $domain <fury-server binary> <commit> with FURY_SSH_KEY=${deploy_key%.pub})
 4. Admin shell: ssh -i $admin_identity ops@$domain
 5. Health check: set GAME_SERVER_URL=https://$domain in wrangler.jsonc vars.
EOF
