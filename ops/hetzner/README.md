# The server

plumblinebible.org is a Hetzner Cloud box in Helsinki, serving
`apps/web/dist` with Caddy, so the app sits under EU jurisdiction and no single
US company's decision can take it down. `plumblinebible.org` and
`www.plumblinebible.org` both point at it (A and AAAA), and so do
`plumblinebible.ch` and `www.plumblinebible.ch` (registered and DNS-hosted at
Infomaniak since 2026-10), a second way in served from the same root. Each
name is its own browser origin, so a reader's notes and settings on one do not
appear on the other; share links and QR codes always name the .org. Nothing on it is
private: the box holds the public bundle and the public data pack, and the
release workflow rebuilds both from source.

GitHub Pages hosted the site until 2026-10 and is still deployed on every
release as a standby (see [Falling back to Pages](#falling-back-to-pages)).

## The box

One small shared-vCPU server (CPX02, 1 GB is plenty: Caddy serving static
files), Debian 13, public IPv4 + IPv6, a Hetzner firewall allowing inbound TCP
22/80/443 only, built from [cloud-init.yaml](cloud-init.yaml) pasted into the
console's "Cloud config" field. What that gives:

- a `deploy` user, key-only; root locked, passwords off, forwarding off,
  `deploy` the only user sshd admits
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
  an update lands, the version header stripped

Replace the two keys under `ssh_authorized_keys` when rebuilding. A rebuilt box
gets a new IP, so update the DNS records (the .org's and, at Infomaniak, the
.ch's), the `MIRROR_HOST` and
`MIRROR_KNOWN_HOSTS` secrets, and `expect_addrs` in
[check-site.sh](check-site.sh); Caddy fetches new certificates once DNS points
at it.

## Releases land here on their own

`.github/workflows/release.yml` checksums every file in `dist` and attaches
the list to the GitHub Release as `SHA256SUMS`, rsyncs `dist` to the box, runs
[check-site.sh](check-site.sh) to prove the site is serving exactly those
bytes, then deploys the Pages standby. It needs three repository secrets;
without them the upload and check are skipped, so a fork publishes to Pages
alone. The `MIRROR_` names date from when this box was a mirror.

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

## Knowing it is up

`.github/workflows/site-check.yml` runs the same check every morning (07:17
UTC, after the box's unattended upgrade) and can be run by hand from the
Actions tab. GitHub emails a failure. The .org is checked byte for byte; the
.ch, the same files under another name, for DNS, certificates and every
manifest file answering. The .ch check is advisory, in the release too: it
depends on a registry outside our control, so its failure shows as a warning
on the run and never fails it. It fails when:

- either name stops resolving to exactly the box's two addresses (a hijacked
  DNS record, with the box untouched)
- any served file differs from the latest release's `SHA256SUMS`, or is
  missing; the list lives on GitHub, so whoever controls the box cannot
  rewrite it to match
- either certificate is within 14 days of expiry, which means Caddy's renewal
  is failing

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
