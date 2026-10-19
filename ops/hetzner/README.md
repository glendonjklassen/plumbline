# The European mirror

plumblinebible.org is GitHub Pages. This is the same `apps/web/dist`, served
from a Hetzner Cloud box in Helsinki by Caddy, so the reader has a copy under
EU jurisdiction that no single company's decision can take down with the
canonical site. Nothing on it is private: the box holds the public bundle and
the public data pack, and the release workflow rebuilds both from source.

## The box

One small shared-vCPU server (CPX02, 1 GB is plenty: Caddy serving static
files), Debian 13, public IPv4 + IPv6, a Hetzner firewall allowing inbound TCP
22/80/443 only, built from [cloud-init.yaml](cloud-init.yaml) pasted into the
console's "Cloud config" field. What that gives:

- a `deploy` user, key-only; root locked, passwords off, forwarding off,
  `deploy` the only user sshd admits
- ufw default-deny with 22/80/443; fail2ban on sshd
- unattended security upgrades, rebooting at 04:30 UTC when a kernel asks
- Caddy as an unprivileged service, root `/srv/plumbline`, a year of
  immutable caching on the content-addressed `/pack/`, `no-cache` on the shell
  files so an update lands, the version header stripped

Replace the two keys under `ssh_authorized_keys` when rebuilding.

## Releases land here on their own

`.github/workflows/release.yml` rsyncs `dist` to the box after the Pages
deploy, when three repository secrets exist. Without them the step is skipped,
so a fork publishes to Pages alone.

| Secret | What |
|---|---|
| `MIRROR_HOST` | the box's hostname or IP |
| `MIRROR_SSH_KEY` | the PRIVATE half of a key made for this and nothing else, whose public half is in `/home/deploy/.ssh/authorized_keys` |
| `MIRROR_KNOWN_HOSTS` | the box's host keys, so the workflow talks only to it: `ssh-keyscan -H <host> 2>/dev/null` |

To mint the deploy key: `ssh-keygen -t ed25519 -f mirror-ci -N "" -C "plumbline release"`,
put `mirror-ci.pub` on one line of the box's `authorized_keys`, the contents of
`mirror-ci` in the secret, then delete both local files. If it ever leaks,
remove that line and rotate the secret; nothing else is affected.

By hand, the same upload is

```sh
rsync -az --delete --exclude CNAME -e "ssh -i ~/.ssh/<your key>" apps/web/dist/ deploy@<host>:/srv/plumbline/
```

## Giving it a name

The Caddyfile serves `:80` until there is a hostname. With one: point its A
and AAAA records at the box, replace `:80` in `/etc/caddy/Caddyfile` with the
hostname, `sudo systemctl reload caddy`. Caddy fetches and renews the
certificate itself; installing and offline use only work over HTTPS, so until
then the mirror is a smoke test, not a fallback readers can keep.
