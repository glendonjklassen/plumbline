# Adding box two, step by step

[box-two.sh](box-two.sh) does these steps interactively: it asks for the two
values, explains each step, shows the command, runs it on a yes and checks the
result; `--from N` resumes after the console and DNS work. This file is the
same steps for reading or doing by hand.

Everything here is copy-paste. Two values are yours to fill in once the box
exists: `B2V4` (box two's IPv4) and `B2V6` (its IPv6). Nothing else is a
placeholder: `<other box IPv4>` below is literal text inside cloud-init.yaml
that the command replaces for you. Box one is `77.42.10.193` /
`2a01:4f9:c015:c326::1`. Run the commands from a checkout of this repo, on
this branch.

## 1. Infomaniak: build the box

Make the user-data: cloud-init.yaml with the release key copied off box one
and the challenge forward pointed at box one. Paste these three lines as they
are.

```sh
key=$(ssh deploy@77.42.10.193 'sudo grep -o "ssh-ed25519 [^ ]* plumbline release" /home/release/.ssh/authorized_keys')
sed -e "s|CI_DEPLOY_PUBLIC_KEY|$key|" -e 's|<other box IPv4>|77.42.10.193|' ops/hetzner/cloud-init.yaml > /tmp/box2.yaml
grep -c "plumbline release\|77.42.10.193" /tmp/box2.yaml   # must print 2
```

In the Infomaniak Manager: Public Cloud, create a project in a Swiss region,
then:

- Security group: inbound TCP 22, 80, 443; outbound all.
- Key pair: not needed, the user-data carries the keys.
- Instance: image **Debian 13**, the smallest flavour, a network with a
  **public IPv4 and IPv6**, the security group above, and the contents of
  `/tmp/box2.yaml` pasted into the **user data** field. Create it.

Write down `B2V4` and `B2V6` from the instance page. Then, about two minutes
later:

```sh
ssh deploy@B2V4 'systemctl is-active caddy ufw fail2ban && sudo ufw status | head -5'
```

## 2. Box one: forward the challenges it cannot answer

```sh
ops/hetzner/caddyfile.sh B2V4 | ssh deploy@77.42.10.193 'sudo cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak && sudo tee /etc/caddy/Caddyfile >/dev/null && sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy'
```

## 3. Box two: the real hostnames

Its certificates arrive through box one's forward; DNS does not have to know
box two yet.

```sh
ops/hetzner/caddyfile.sh 77.42.10.193 | ssh deploy@B2V4 'sudo tee /etc/caddy/Caddyfile >/dev/null && sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy'
ssh deploy@B2V4 'sudo journalctl -u caddy -n 50 -f'
```

Wait for `certificate obtained successfully` for all four names, then Ctrl-C.

## 4. Fill box two with the live site

```sh
rsync -az deploy@77.42.10.193:/srv/plumbline/ /tmp/site/
rsync -az --rsync-path="sudo -u release rsync" /tmp/site/ deploy@B2V4:/srv/plumbline/
BACKEND=B2V4 ops/hetzner/check-site.sh https://plumblinebible.org
```

The last line must end `OK https://plumblinebible.org via B2V4: ...`.

## 5. DNS: both zones

**Cloudflare**, plumblinebible.org, DNS, Records. Add four, all with proxy
status **DNS only** (grey cloud) and TTL **1 min**:

| Type | Name | Content |
|---|---|---|
| A | `plumblinebible.org` | `B2V4` |
| AAAA | `plumblinebible.org` | `B2V6` |
| A | `www` | `B2V4` |
| AAAA | `www` | `B2V6` |

Edit the four existing records (box one's) to TTL 1 min as well. DNSSEC
stays on.

**Infomaniak**, Domains, plumblinebible.ch, DNS zone. The same four records,
TTL 60, and the existing four to TTL 60.

Verify, a minute later:

```sh
getent ahosts plumblinebible.org | awk '{print $1}' | sort -u
getent ahosts plumblinebible.ch | awk '{print $1}' | sort -u
```

Each prints the four addresses.

## 6. GitHub: the secrets and the checker

```sh
gh secret set MIRROR2_HOST --body B2V4
ssh-keyscan -H 77.42.10.193 B2V4 2>/dev/null | gh secret set MIRROR_KNOWN_HOSTS
sed -i 's|^expect_addrs=.*|expect_addrs=${EXPECT_ADDRS:-77.42.10.193 2a01:4f9:c015:c326::1 B2V4 B2V6}|' ops/hetzner/check-site.sh
ops/hetzner/check-site.sh https://plumblinebible.org
git commit -am "Box two is in DNS and the checker expects it" && git push
```

If `MIRROR_HOST` is a hostname rather than `77.42.10.193`, scan that name
instead of the address on the second line. Merge the PR and tag the release;
the site job uploads to both boxes and checks each by address.

## 7. Optional: Cloudflare health checks on the .org

Cloudflare, plumblinebible.org, Traffic, Load Balancing, enable (the base
plan). Then:

- **Monitor**: type HTTPS, path `/`, expected code 200, header
  `Host: plumblinebible.org`, interval 60 s.
- **Pool**: name `boxes`, origins `77.42.10.193` and `B2V4`, that monitor.
- **Load balancer**: hostname `plumblinebible.org`, pool `boxes`,
  **proxy off**, steering random. A second one for `www.plumblinebible.org`.

Delete the plain A and AAAA records for those two names afterwards; the load
balancer answers for them with whichever addresses are healthy. Leave the .ch
alone: it fails differently, which is the point.

## 8. The three logins

Two-factor with a hardware key on Cloudflare, Infomaniak and Hetzner. Those
accounts are the only thing that can take both names away at once.
