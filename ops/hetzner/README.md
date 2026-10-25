# The servers

plumblinebible.org is two boxes serving `apps/web/dist` with Caddy: one at
Hetzner Cloud in Helsinki, one at Infomaniak Public Cloud in Switzerland. Both
names carry both boxes' addresses (A and AAAA), so a browser that one box
refuses retries the other on its own, and no one company's outage or decision
takes the site down. `plumblinebible.org` is registered and DNS-hosted at
Cloudflare (DNS only, never proxied: Caddy on the boxes terminates TLS);
`plumblinebible.ch` at Infomaniak (since 2026-10), a second way in served from
the same root, under a different registry, registrar and DNS host, so a name
survives what takes the other. Each name is its own browser origin, so a
reader's notes and settings on one do not appear on the other; share links and
QR codes name the .org unless the reader picked the .ch on the Share screen,
and About lists both. Nothing on a box is private: it holds the public bundle
and the public data pack, and the release workflow rebuilds both from source.

GitHub Pages hosted the site until 2026-10 and is still deployed on every
release as a standby (see [Falling back to Pages](#falling-back-to-pages)).

## The box

Both boxes are the same build: one small shared-vCPU server (a Hetzner CPX02,
the smallest Infomaniak flavour; 1 GB is plenty: Caddy serving static files),
Debian 13, public IPv4 + IPv6, the provider's firewall or security group
allowing inbound TCP 22/80/443 only, built from
[cloud-init.yaml](cloud-init.yaml) pasted into the console's cloud-config /
user-data field. What that gives:

- a `deploy` user (the maintainer, sudo) and a `release` user (owns the web
  root, no sudo), both key-only; root locked, passwords off, forwarding off,
  those two the only users sshd admits. The release workflow's key is in
  `release` alone and bound to `rrsync -wo /srv/plumbline`: it can rsync into
  the web root and do nothing else — not read it back, not open a shell, and
  with no sudo on the account, not reach the rest of the box through a flaw in
  the rsync receiver either
- ufw default-deny with 22/80/443; fail2ban on sshd
- unattended security upgrades, rebooting at 04:30 UTC when a kernel asks
- Caddy from its own apt repository (key pinned in the cloud-init), which the
  unattended upgrades follow too, so it stays current rather than on Debian's
  version
- Caddy sandboxed by a systemd drop-in: read-only filesystem except
  `/var/lib/caddy` (its certificates), no new privileges, service syscalls only
- Caddy as an unprivileged service, root `/srv/plumbline`, HTTPS for all four
  names with certificates it fetches and renews itself, a year of immutable
  caching on the content-addressed `/pack/`, `no-cache` on the shell files so
  an update lands, the version header stripped, HSTS, and a Content-Security-Policy
  that allows the app's own origin and nothing else (see
  [The headers](#the-headers))

Replace the two keys under `ssh_authorized_keys` and the `<other box IPv4>`
in the Caddyfile when rebuilding. A rebuilt box gets new addresses, so: take
the old ones out of both zones (Cloudflare for the .org, Infomaniak for the
.ch) and put the new ones in, update that box's host secret (`MIRROR_HOST` or
`MIRROR2_HOST`) and its lines in `MIRROR_KNOWN_HOSTS`, update `expect_addrs`
in [check-site.sh](check-site.sh), and update the forward address in the
OTHER box's Caddyfile. Then copy the live site onto it before it goes into
DNS (below); Caddy fetches its certificates as soon as it is reloaded with
the real hostnames, through the other box's forward.

## Two boxes

Adding the second box, as copy-paste steps: [BOX-TWO.md](BOX-TWO.md). Two
things need saying about it that one box never needed.

**Certificates.** Each box fetches its own, and a certificate challenge
arrives at whichever box DNS hands out. Caddy answers its own challenge tokens
on port 80 before any route, and the Caddyfile's plain-HTTP block forwards
the tokens it does not know to the other box, which does. So each box names
the other's IPv4 there, and a box can get its certificates before DNS names
it at all: every challenge lands on the box DNS does name and is forwarded.
`journalctl -u caddy -f` shows the four names being obtained.

**The first copy.** A box in DNS with nothing in `/srv/plumbline` serves 404s
to half the readers, so a new box is filled before its records go in, by hand
from the live one (the release user cannot read the site back, so this goes
through `deploy` both ways):

```sh
rsync -az deploy@<live box>:/srv/plumbline/ /tmp/site/
rsync -az --rsync-path="sudo -u release rsync" /tmp/site/ deploy@<new box>:/srv/plumbline/
BACKEND=<new box IPv4> ops/hetzner/check-site.sh https://plumblinebible.org
```

After that the release keeps both boxes identical: it uploads to every box
before checking any (a reader round-robined between a new shell and an old
box's assets is a broken boot), and both uploads must succeed. A box that is
gone comes out of both zones by hand and the job is re-run; the TTL on the
address records is 60 s on both zones for that reason.

## Releases land here on their own

`.github/workflows/release.yml` checksums every file in `dist` and attaches
the list to the GitHub Release as `SHA256SUMS`, rsyncs `dist` to every box,
runs [check-site.sh](check-site.sh) once per box, pinned to its address, to
prove each is serving exactly those bytes, then deploys the Pages standby. It
needs the repository secrets below; without them the upload and check are
skipped, so a fork publishes to Pages alone. The `MIRROR_` names date from
when the first box was a mirror.

| Secret | What |
|---|---|
| `MIRROR_HOST` | the first box's hostname or IP |
| `MIRROR2_HOST` | the second box's; empty means one box |
| `MIRROR_SSH_KEY` | the PRIVATE half of one key made for this and nothing else, whose public half is in `/home/release/.ssh/authorized_keys` on every box, bound to rrsync as below |
| `MIRROR_KNOWN_HOSTS` | every box's host keys, so the workflow talks only to them: `ssh-keyscan -H <host> 2>/dev/null` for each, concatenated |

To mint the deploy key: `ssh-keygen -t ed25519 -f mirror-ci -N "" -C "plumbline release"`,
put `mirror-ci.pub` on one line of `/home/release/.ssh/authorized_keys` with
the restriction in front of it, the contents of `mirror-ci` in the secret, then
delete both local files:

```
restrict,command="/usr/bin/rrsync -wo /srv/plumbline" ssh-ed25519 AAAA... plumbline release
```

`restrict` is no forwarding and no pty; rrsync (from the rsync package)
refuses every command but an rsync server rooted at `/srv/plumbline`, and
`-wo` refuses to send anything back. A leaked key can overwrite the public
site and nothing else, and the next release overwrites it again. rrsync reads
every path relative to that root, which is why the workflow's target is
`release@<host>:/`. If the key ever leaks, remove that line and rotate the
secret; nothing else is affected.

### Moving the release key to its own user (a box built before 2026-10-23)

The key used to sit in `deploy`'s `authorized_keys`, beside the maintainer's,
on an account with passwordless sudo. On such a box, as `deploy`, before
tagging a release that targets `release@`:

```sh
sudo adduser --disabled-password --gecos "" --shell /bin/sh release
sudo install -d -m 700 -o release -g release /home/release/.ssh
grep 'plumbline release$' ~/.ssh/authorized_keys | sudo install -m 600 -o release -g release /dev/stdin /home/release/.ssh/authorized_keys
sudo cat /home/release/.ssh/authorized_keys          # one line, starting restrict,command=
sed -i '/plumbline release$/d' ~/.ssh/authorized_keys
sudo chown -R release:release /srv/plumbline
sudo sed -i 's/^AllowUsers deploy$/AllowUsers deploy release/' /etc/ssh/sshd_config.d/10-hardening.conf
sudo sshd -t && sudo systemctl reload ssh
```

Then, from a second terminal, confirm `ssh deploy@<host>` still works before
closing the first. The secrets do not change: same key, same host. The first
release after this is the test of it; an upload refused there is this list
done in the wrong order, and `gh run rerun <run-id> --failed` resumes once the
box is right.

By hand, with your own (unrestricted) key, the same upload is

```sh
rsync -az --delete --exclude CNAME --rsync-path="sudo -u release rsync" -e "ssh -i ~/.ssh/<your key>" apps/web/dist/ deploy@<host>:/srv/plumbline/
```

## The headers

The Caddyfile in [cloud-init.yaml](cloud-init.yaml) is the source for the
security headers, and the box is held to it: [check-site.sh](check-site.sh)
reads the `Content-Security-Policy` out of that file and fails unless the site
serves exactly it, plus HSTS and nosniff, on `/` and on a 404 (Caddy's error
handler is a second chain, which is why the `(security)` snippet is imported
twice). `apps/web/vite.config.ts` reads the same line and `vite preview`
serves it, so the whole Playwright suite runs under the production policy and
`apps/web/e2e/csp.spec.ts` re-derives the two script hashes in it from the
build.

So changing a header is: edit cloud-init.yaml, make the suite pass, then put
the new Caddyfile on the box before merging, or the next daily check fails:

```sh
ops/hetzner/caddyfile.sh <other box IPv4> | ssh deploy@<host> 'sudo cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak && sudo tee /etc/caddy/Caddyfile >/dev/null && sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy'
BACKEND=<host> ops/hetzner/check-site.sh https://plumblinebible.org   # headers, DNS, availability
```

on each box. [caddyfile.sh](caddyfile.sh) prints the `content:` block of
cloud-init.yaml with `:80` replaced by the four hostnames and the challenge
forward pointed at the other box, which is exactly what a live box runs. The
inline scripts in `apps/web/index.html` (the
theme) and `apps/web/public/404.html` (the redirect) are the two hashes in the
policy; edit either and its hash must follow (the spec prints the new one).

## Knowing it is up

`.github/workflows/site-check.yml` runs the same check every morning (07:17
UTC, after the boxes' unattended upgrade), once per box, and can be run by
hand from the Actions tab. GitHub emails a failure. It checks the .org only:
the .ch is the same boxes and files under another name, so the .org passing
covers it. It fails when:

- either name stops resolving to exactly the boxes' four addresses (a hijacked
  DNS record, with the boxes untouched)
- any served file differs from the latest release's `SHA256SUMS`, or is
  missing; the list lives on GitHub, so whoever controls the box cannot
  rewrite it to match
- either certificate is within 14 days of expiry, which means Caddy's renewal
  is failing
- the served security headers differ from cloud-init.yaml's, on `/` or on a
  404 (a hand edit on the box, or a Caddy upgrade changing how `header`
  composes)

It cannot catch a box that serves GitHub's runners the real files and everyone
else something else; the hardening above is the defence against that. For
warning of a certificate issued for the domain anywhere else, sign the domain
up for a certificate-transparency alert (Cert Spotter's is free).

Locally, with or without a release's checksums:

```sh
gh release download --pattern SHA256SUMS --dir /tmp/plumbline-sums
ops/hetzner/check-site.sh https://plumblinebible.org /tmp/plumbline-sums/SHA256SUMS
ops/hetzner/check-site.sh https://plumblinebible.org   # availability only
```

## Falling back to Pages

If the box is gone and a rebuild will take a while, point DNS back at GitHub
Pages: A records `185.199.108.153`, `185.199.109.153`, `185.199.110.153`,
`185.199.111.153` for the apex, and `www` as a CNAME to
`glendonjklassen.github.io`. The repository's Pages settings still carry the
custom domain, and every release has kept that copy current.
