#!/usr/bin/env bash
# Adds box two, interactively: the steps of BOX-TWO.md, each explained, shown
# as the exact command, run on a yes, and checked. Asks for the two values
# that are yours (box two's IPv4 and IPv6) and nothing else.
#
#   ops/hetzner/box-two.sh            # from the top
#   ops/hetzner/box-two.sh --from 5   # resume at a step (after the console or DNS work)
#
# Needs: ssh to deploy@ on both boxes, rsync, gh (logged in), git.
set -euo pipefail

B1V4=77.42.10.193
B1V6=2a01:4f9:c015:c326::1
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
CADDY_APPLY='sudo cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak 2>/dev/null; sudo tee /etc/caddy/Caddyfile >/dev/null && sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy'

from=1
[ "${1:-}" = "--from" ] && from=${2:?--from needs a step number}

# ── talking ──
say()  { printf '\n%s\n' "$*"; }
step() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }
die()  { printf '\n\033[31m%s\033[0m\n' "$*" >&2; exit 1; }
ok()   { printf '\033[32m  ok: %s\033[0m\n' "$*"; }
# The prompt goes to stderr: the answer is what $(ask ...) captures.
ask()  { local a; printf '%s ' "$1" >&2; read -r a; printf '%s' "$a"; }
# Show the command, ask, run it. "s" skips (already done), anything else but y/Y/Enter stops.
run() {
  printf '\n  $ %s\n' "$1"
  local a; printf '  run it? [Y/n/s(kip)] '; read -r a
  case $a in
    s|S) echo "  skipped"; return 0 ;;
    ""|y|Y) eval "$1" ;;
    *) die "stopped at your request; resume with: $0 --from $STEP" ;;
  esac
}
pause() { printf '\n%s\n  [Enter when done] ' "$1"; read -r _; }
# A command that fails under set -e lands here: say where to pick up.
trap 'printf "\n\033[31mfailed in step %s; fix it and resume with: %s --from %s\033[0m\n" "$STEP" "$0" "$STEP" >&2' ERR
STEP=0

# ── the two values ──
say "Box one is $B1V4 / $B1V6 (Hetzner). Box two is the Infomaniak instance."
B2V4=$(ask "Box two IPv4 (empty if it is not built yet):")
if [ -n "$B2V4" ]; then
  [[ $B2V4 =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "that is not an IPv4 address"
  B2V6=$(ask "Box two IPv6:")
  [[ $B2V6 == *:* ]] || die "that is not an IPv6 address"
fi

STEP=1
if [ "$from" -le 1 ]; then
  step "1. Infomaniak: the user-data for box two"
  say "cloud-init.yaml builds the box. Two things in it are per box: the release
user's public key (CI_DEPLOY_PUBLIC_KEY), which must be the SAME key box one
has so the release workflow keeps one secret, and the address of the other
box, which Caddy forwards certificate challenges to. This reads the key off
box one and writes the finished file to /tmp/box2.yaml."
  run "key=\$(ssh deploy@$B1V4 'sudo grep -o \"ssh-ed25519 [^ ]* plumbline release\" /home/release/.ssh/authorized_keys') && sed -e \"s|CI_DEPLOY_PUBLIC_KEY|\$key|\" -e 's|<other box IPv4>|$B1V4|' '$HERE/cloud-init.yaml' > /tmp/box2.yaml"
  n=$(grep -c "plumbline release\|$B1V4" /tmp/box2.yaml || true)
  [ "$n" = 2 ] || die "/tmp/box2.yaml should mention the key and $B1V4 once each (found $n lines)"
  ok "/tmp/box2.yaml is the user-data"
  say "Now in the Infomaniak Manager, Public Cloud, a project in a Swiss region:
  - Security group: inbound TCP 22, 80, 443; outbound all.
  - Instance: image Debian 13, the smallest flavour, a network with a PUBLIC
    IPv4 and IPv6, that security group, and the contents of /tmp/box2.yaml in
    the user-data field.
Write down its IPv4 and IPv6 from the instance page."
  if [ -z "$B2V4" ]; then
    say "Build it, then come back: $0 --from 2"
    exit 0
  fi
fi

STEP=2
if [ "$from" -le 2 ]; then
  step "2. Box two is up"
  say "cloud-init takes about two minutes. The first ssh will ask you to accept
the box's host key; say yes, that is the one time it is unverified."
  until ssh -o ConnectTimeout=5 "deploy@$B2V4" 'systemctl is-active caddy ufw fail2ban >/dev/null && sudo ufw status | head -5'; do
    [ "$(ask "  not answering yet; try again? [Y/n]")" = n ] && die "box two is not reachable; resume with: $0 --from 2"
  done
  ok "caddy, ufw and fail2ban are running on box two"
fi

STEP=3
if [ "$from" -le 3 ]; then
  step "3. Box one: forward the challenges it cannot answer"
  say "Each box fetches its own certificates, and a challenge lands on whichever
box DNS hands out. Caddy answers its own tokens first; the plain-HTTP block in
the Caddyfile forwards the rest to the other box. This is box one's Caddyfile
with that block pointed at box two. The diff against what it runs now:"
  "$HERE/caddyfile.sh" "$B2V4" > /tmp/caddy-box1
  ssh "deploy@$B1V4" 'sudo cat /etc/caddy/Caddyfile' > /tmp/caddy-box1.now || true
  diff /tmp/caddy-box1.now /tmp/caddy-box1 || true
  run "'$HERE/caddyfile.sh' $B2V4 | ssh deploy@$B1V4 '$CADDY_APPLY'"
  ok "box one reloaded"
fi

STEP=4
if [ "$from" -le 4 ]; then
  step "4. Box two: the real hostnames"
  say "Box two's Caddyfile still says ':80' (plain HTTP, no names). This puts the
four names in, with the forward pointed at box one. Caddy then asks for its
certificates; DNS does not name box two yet, so every challenge reaches box
one and is forwarded here. Expect all four names within a minute or two."
  run "'$HERE/caddyfile.sh' $B1V4 | ssh deploy@$B2V4 '$CADDY_APPLY'"
  say "Waiting for the four certificates (box two's Caddy log):"
  for _ in $(seq 1 30); do
    got=$(ssh "deploy@$B2V4" 'sudo journalctl -u caddy --since "-15min" --no-pager 2>/dev/null' |
      grep 'certificate obtained successfully' | grep -o '"identifier":"[^"]*"' | sort -u | wc -l)
    printf '  %s of 4\n' "$got"
    [ "$got" -ge 4 ] && break
    sleep 10
  done
  if [ "$got" -lt 4 ]; then
    ssh "deploy@$B2V4" 'sudo journalctl -u caddy --since "-15min" --no-pager | tail -20'
    die "only $got certificates after five minutes; the log above says why. Resume with: $0 --from 4"
  fi
  ok "box two holds certificates for all four names"
fi

STEP=5
if [ "$from" -le 5 ]; then
  step "5. Fill box two with the live site"
  say "A box in DNS with an empty web root serves 404s to half the readers, so
box two is filled from box one before its records go in. The release user
cannot read the site back (rrsync is write-only), so this goes through deploy
both ways: box one to this PC, this PC to box two."
  run "rsync -az --info=progress2 deploy@$B1V4:/srv/plumbline/ /tmp/site/"
  run "rsync -az --info=progress2 --rsync-path='sudo -u release rsync' /tmp/site/ deploy@$B2V4:/srv/plumbline/"
  say "The checker, pinned to box two by address (DNS still names only box one):"
  run "BACKEND=$B2V4 '$HERE/check-site.sh' https://plumblinebible.org"
  ok "box two serves the site"
fi

STEP=6
if [ "$from" -le 6 ]; then
  step "6. DNS: both zones"
  say "Cloudflare, plumblinebible.org, DNS, Records. Add four, proxy status
DNS ONLY (grey cloud), TTL 1 min; and set the existing four to TTL 1 min:
    A     plumblinebible.org   $B2V4
    AAAA  plumblinebible.org   $B2V6
    A     www                  $B2V4
    AAAA  www                  $B2V6
Infomaniak, Domains, plumblinebible.ch, DNS zone: the same four, TTL 60, and
the existing four to TTL 60."
  pause "Add them."
  want=$(printf '%s\n' "$B1V4" "$B1V6" "$B2V4" "$B2V6" | sort -u | tr '\n' ' ')
  for name in plumblinebible.org www.plumblinebible.org plumblinebible.ch www.plumblinebible.ch; do
    for _ in $(seq 1 30); do
      got=$(getent ahosts "$name" | awk '{print $1}' | sort -u | tr '\n' ' ')
      [ "$got" = "$want" ] && break
      printf '  %s: [%s] (waiting for the TTL)\n' "$name" "$got"
      sleep 10
    done
    [ "$got" = "$want" ] || die "$name resolves to [$got], expected [$want]; fix the zone and resume with: $0 --from 6"
    ok "$name resolves to all four"
  done
fi

STEP=7
if [ "$from" -le 7 ]; then
  step "7. GitHub: the secrets and the checker"
  say "The release uploads to every box named by a secret, and the checker
expects both names to resolve to exactly the boxes' addresses. MIRROR_HOST is
box one as the secret names it (an address or a hostname); it is rescanned so
MIRROR_KNOWN_HOSTS holds both boxes' host keys."
  mh=$(ask "MIRROR_HOST as stored in the repository secrets [$B1V4]:")
  mh=${mh:-$B1V4}
  run "gh secret set MIRROR2_HOST --body $B2V4"
  run "ssh-keyscan -H $mh $B2V4 2>/dev/null | gh secret set MIRROR_KNOWN_HOSTS"
  run "sed -i 's|^expect_addrs=.*|expect_addrs=\${EXPECT_ADDRS:-$B1V4 $B1V6 $B2V4 $B2V6}|' '$HERE/check-site.sh'"
  grep -n '^expect_addrs=' "$HERE/check-site.sh"
  say "The checker, unpinned, so the DNS test runs against the new expectation:"
  run "'$HERE/check-site.sh' https://plumblinebible.org"
  run "cd '$ROOT' && git add ops/hetzner/check-site.sh && git commit -m 'Box two is in DNS and the checker expects it' && git push"
  ok "secrets set, checker updated, pushed"
fi

step "Done"
say "Merge the PR and tag the release: the site job uploads to both boxes and
checks each by address. Two things left, both optional and both by hand:
  - Health-checked failover on the .org: Cloudflare, Traffic, Load Balancing
    (BOX-TWO.md step 7). Proxy OFF on the load balancer, or Cloudflare becomes
    the one company in every request.
  - Two-factor with a hardware key on Cloudflare, Infomaniak and Hetzner: the
    three logins are the only thing that can take both names away at once."
