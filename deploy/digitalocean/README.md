# DigitalOcean host

Production runs on one DigitalOcean droplet (owner decision 2026-11-13:
Hetzner's US prices rose to about $20.50 a month; no Hetzner script is kept or
used). Everything outside this directory is provider neutral.

| Resource | Value |
|---|---|
| Project | `fury-game` (must exist; the droplet and Reserved IP are assigned to it; DigitalOcean does not list firewalls under projects) |
| Droplet | `fury-game`, `s-1vcpu-1gb` ($6/month), `ubuntu-24-04-x64`, monitoring agent, IPv6, tag `fury-game` |
| Region | `nyc2` by default: on 2026-11-13 `s-1vcpu-1gb` was not offered in `nyc1`/`nyc3` (the script checks and lists where it is) |
| Cloud Firewall | `fury-game`, applied by tag: in 443/tcp, 443/udp (WebTransport later), 22/tcp, ICMP; all outbound |
| Reserved IP | the stable address DNS points at (free while assigned; DigitalOcean bills an unassigned one) |
| SSH keys | `fury-game-admin` (root, which cannot log in, and the `ops` user), `fury-game-deploy` (uploaded for the record; only the `deploy` user gets it, restricted) |

## Provision

Needs `doctl` authenticated (`DIGITALOCEAN_ACCESS_TOKEN` or the active
context), `~/.ssh/fury_game_admin{,.pub}` and `~/.ssh/fury_game_deploy.pub`.

```sh
deploy/digitalocean/provision.sh            # create what is missing, check the rest
deploy/digitalocean/provision.sh --replace  # fresh droplet, Reserved IP moved to it
```

Options: `--name`, `--region`, `--size`, `--project`, `--admin-key`,
`--deploy-key`, `--domain` (default `mp.furyremastered.com`), `--acme-email`.

Each run reapplies the firewall rules, creates the droplet only if it is
missing, waits for cloud-init over admin SSH and checks the result (release
helper installed, unit enabled, effective sshd settings, directory owners),
keeps the Reserved IP on the droplet, and prints next steps including the
host key line for `GAME_SERVER_KNOWN_HOSTS`.

`--replace` creates the new droplet first, waits for cloud-init, moves the
Reserved IP, then deletes the old droplet. It has a short outage: the new
droplet has no release until the next deploy (`gh workflow run
game-server.yml --ref main`), it requests a new certificate, and its SSH host
key changes, so update `GAME_SERVER_KNOWN_HOSTS` from the printed line first.
Cloud-init changes (unit, helper, sysctls) reach the host only through
`--replace`.

## One-time setup after the first run

1. Cloudflare DNS, **DNS only** (grey cloud): `A mp -> <Reserved IP>`. The
   server terminates TLS itself, so the record must not be proxied.
2. GitHub repository secrets:
   - `GAME_SERVER_HOST` = `mp.furyremastered.com`
   - `GAME_SERVER_SSH_KEY` = contents of `~/.ssh/fury_game_deploy` (`gh secret set GAME_SERVER_SSH_KEY < ~/.ssh/fury_game_deploy`)
   - `GAME_SERVER_KNOWN_HOSTS` = the line provision.sh prints (`mp.furyremastered.com,<Reserved IP> ssh-ed25519 ...`)
3. Deploy once the server crate is on main: `gh workflow run game-server.yml --ref main`.
4. Set `GAME_SERVER_URL` to `https://mp.furyremastered.com` in `wrangler.jsonc`.
