# Deploying fury-server

`fury-server` (crates/atv-server, design in
[research/GAME_SERVER.md](../research/GAME_SERVER.md)) is one static Linux
binary. Everything here except `digitalocean/` is provider neutral: any Ubuntu
24.04 VPS that accepts cloud-init user data, or any container host, works.

| File | What it does |
|---|---|
| `cloud-init.yaml` | First boot: users, directories, SSH hardening, unattended upgrades (reboot 09:30 UTC when needed), socket sysctls, journald cap, the unit and release helper. Placeholders are filled by `render-cloud-init.sh`. |
| `render-cloud-init.sh` | `render-cloud-init.sh <admin.pub> <deploy.pub> [--domain D] [--acme-email E]` prints ready user data. |
| `systemd/fury-server.service` | Hardened unit: user `fury`, only `CAP_NET_BIND_SERVICE`, read-only system except `/var/lib/fury-server`, `Restart=always`, 15 s stop timeout. |
| `env.example` | `/etc/fury-server/env` (no secrets; TLS is Let's Encrypt in process). |
| `fury-release` | Server side of deploys, installed as `/usr/local/bin/fury-release`; the deploy key's only command. |
| `deploy.sh` | `deploy.sh <host> <binary> <commit>`: upload, `--version` check, switch, restart, wait for `/healthz` to report the commit, roll back on failure, keep 5 releases. |
| `test_deploy.py` | deploy.sh and fury-release against a stand-in host (stub ssh, local `/healthz`). |
| `Dockerfile` | The same binary on distroless for any container host. |
| `digitalocean/` | The production host: `provision.sh` creates the droplet, firewall and Reserved IP with `doctl`. |

## Host layout

```
/opt/fury-server/releases/<commit>/fury-server   uploaded by deploy (owner deploy)
/opt/fury-server/releases/<commit>/release.env   FURY_COMMIT=<commit>
/opt/fury-server/current -> releases/<commit>    what systemd runs
/opt/fury-server/previous -> releases/<commit>   the rollback target
/var/lib/fury-server/{state,acme}                saved rooms, certificates (owner fury)
/etc/fury-server/env                             settings (root:fury 0640)
```

Users: `ops` (owner's key, sudo; Ubuntu reserves the name `admin` for a group), `deploy` (key restricted by
`authorized_keys` to `fury-release`, which can upload, activate, roll back,
prune and read the service log; sudo only for `systemctl restart
fury-server.service`), `fury` (runs the service, no login). Root and password
logins are off. Nothing on the host needs a backup: rooms survive restarts via
`/var/lib/fury-server/state`, and a lost host is rebuilt with one command.

## Deploying

CI (`.github/workflows/game-server.yml`) builds `x86_64-unknown-linux-musl`,
runs the server's tests and, on main, deploys when the secrets
`GAME_SERVER_HOST`, `GAME_SERVER_SSH_KEY` and `GAME_SERVER_KNOWN_HOSTS` exist.
Deploys are serialized and never cancelled; a build older than the live commit
is skipped. Run it by hand with `gh workflow run game-server.yml --ref main`.

From a Mac with the deploy key:

```sh
FURY_SSH_KEY=~/.ssh/fury_game_deploy deploy/deploy.sh mp.furyremastered.com path/to/fury-server "$(git rev-parse HEAD)"
```

The binary must answer `fury-server --version` (exit 0) without starting the
server; that is the pre-activation check. A failed health check prints the
service log and restores the previous release.

Admin shell: `ssh -i ~/.ssh/fury_game_admin ops@mp.furyremastered.com`
(`journalctl -u fury-server -f`, `sudo systemctl status fury-server`).

## Monitoring

The site Worker checks `${GAME_SERVER_URL}/healthz` every five minutes
(`cloudflare/gameserver.mjs`, D1 table `game_server_checks`, 30 days) and
serves `https://furyremastered.com/status/game-server`. Set `GAME_SERVER_URL`
in `wrangler.jsonc` once the server answers. With a `GITHUB_ALERT_TOKEN`
Worker secret (fine-grained, Issues read/write on this repository) two failed
checks in a row open one `area: infra` issue, closed again on recovery.
