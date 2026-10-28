#!/bin/sh
# Checks that the live site serves what the release built. Run by the release
# after every upload, and daily by .github/workflows/site-check.yml, because
# the server changes between releases too: Caddy and Debian update themselves,
# the certificate renews itself, and a compromised box would change files
# without a release.
#
#   check-site.sh https://plumblinebible.org [SHA256SUMS]
#   BACKEND=<box> check-site.sh ...   # every request pinned to that one box
#
# - DNS: both names must resolve to exactly EXPECT_ADDRS, so a hijacked record
#   fails here even though the box itself is untouched.
# - BACKEND (a hostname or IPv4): every request below goes to that box rather
#   than to whichever address DNS hands out. With two boxes an unpinned run
#   samples one of them, so the release runs this once per box.
# - With SHA256SUMS (the release writes one from `dist` and attaches it to the
#   GitHub Release, out of the box's reach): every file is downloaded and must
#   match it byte for byte. Any edit to any served file fails, not only a
#   missing one. Without it: every file the shell and pack manifests name must
#   answer, which is availability only.
# - The certificate must not be within MIN_CERT_DAYS of expiry: Caddy renews
#   well before that, so reaching it means renewal is failing.
#
# What no outside check can see: a box that serves GitHub's runners the real
# files and everyone else something else.
set -eu

origin=${1%/}
sums=${2:+$(realpath "$2")}
host=${origin#https://}
min_days=${MIN_CERT_DAYS:-14}

fail() { echo "::error::$*"; exit 1; }
# Every box's addresses (ops/hetzner/README.md): both names must carry exactly
# these. A rebuild changes a box's pair; a box added to DNS is added here in the
# same change, or the next release fails on this line.
expect_addrs=${EXPECT_ADDRS:-77.42.10.193 2a01:4f9:c015:c326::1}

# The one box this run proves, if any. A name is resolved to its IPv4 once, here,
# so every request below reaches the same machine.
backend=${BACKEND:-}
resolve=""
if [ -n "$backend" ]; then
  case $backend in
    *[!0-9.]*) backend=$(getent ahostsv4 "$backend" | awk 'NR==1{print $1}') ;;
  esac
  [ -n "$backend" ] || fail "BACKEND=$BACKEND: cannot resolve"
  resolve="--resolve $host:443:$backend --resolve www.$host:443:$backend"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

addrs() {
  if command -v dig >/dev/null; then
    { dig +short A "$1"; dig +short AAAA "$1"; } | grep -E '^[0-9.]+$|:'
  else
    getent ahosts "$1" | awk '{print $1}'
  fi | sort -u | tr '\n' ' '
}

want=$(printf '%s\n' $expect_addrs | sort -u | tr '\n' ' ')
for name in "$host" "www.$host"; do
  got=$(addrs "$name")
  [ "$got" = "$want" ] || fail "$name: resolves to [${got% }], expected [${want% }]"
done

# Retried: the one request a restarting Caddy could plausibly miss.
get() {
  for try in 1 2 3; do
    curl -fsS $resolve --max-time 30 -o "$tmp/$2" "$origin/$1" && return 0
    [ "$try" = 3 ] || sleep 10
  done
  fail "$origin/$1: no response"
}
get "" index.html

# The security headers are the box's own configuration, pasted from
# cloud-init.yaml by hand, and this is what holds the two equal: the policy is
# read out of that file, not copied here. Checked on / and on a 404, because
# Caddy's error handler is a second chain that used to drop every header
# (2026-10-22). HSTS and nosniff only have to be present; the CSP has to be
# the exact string, since one dropped directive is a silent weakening.
want_csp=$(sed -n 's/^ *Content-Security-Policy "\(.*\)"$/\1/p' "$(dirname "$(realpath "$0")")/cloud-init.yaml")
[ -n "$want_csp" ] || fail "cloud-init.yaml: no Content-Security-Policy line"
for path in "" "no-such-page-$$"; do
  curl -sS $resolve --max-time 30 -D "$tmp/headers" -o /dev/null "$origin/$path" ||
    fail "$origin/$path: no response"
  got_csp=$(tr -d '\r' < "$tmp/headers" | sed -n 's/^[Cc]ontent-[Ss]ecurity-[Pp]olicy: //p')
  [ "$got_csp" = "$want_csp" ] ||
    fail "$origin/$path: Content-Security-Policy [$got_csp] does not match cloud-init.yaml [$want_csp]"
  grep -qi '^strict-transport-security: max-age=[1-9]' "$tmp/headers" ||
    fail "$origin/$path: no Strict-Transport-Security header"
  grep -qi '^x-content-type-options: nosniff' "$tmp/headers" ||
    fail "$origin/$path: no X-Content-Type-Options header"
done

# The pack manifest is the one file under /pack/ with no hash in its URL, and
# the one the app asks for to learn that a release happened. Served with the
# year-long `immutable` header the rest of /pack/ gets, the browser answers
# that question from its own cache and a data release never reaches an
# installed device (live 2026-10-18 to v0.81.0: the app updated, the data did
# not). The app now revalidates regardless; this holds the box to the same.
curl -sS $resolve --max-time 30 -D "$tmp/headers" -o /dev/null "$origin/pack/manifest.json" ||
  fail "$origin/pack/manifest.json: no response"
got_cc=$(tr -d '\r' < "$tmp/headers" | sed -n 's/^[Cc]ache-[Cc]ontrol: //p')
[ "$got_cc" = "no-cache" ] ||
  fail "$origin/pack/manifest.json: Cache-Control [$got_cc] must be [no-cache] — the box is not running the Caddyfile in cloud-init.yaml"

if [ -n "$sums" ]; then
  # Every file at once, then one comparison that names what differs. No
  # Accept-Encoding is sent, so Caddy serves the stored bytes.
  mkdir "$tmp/site"
  awk -v o="$origin" '{print "url = \"" o "/" $2 "\"\noutput = \"" $2 "\""}' "$sums" > "$tmp/curl.conf"
  (cd "$tmp/site" && curl -sS $resolve --parallel --parallel-max 8 --create-dirs --max-time 120 -K "$tmp/curl.conf") || true
  if ! (cd "$tmp/site" && sha256sum --check --quiet "$sums" 2>&1) > "$tmp/diff"; then
    sed 's/^/::error::/' "$tmp/diff"
    fail "$origin: $(grep -c FAILED "$tmp/diff") file(s) differ from SHA256SUMS"
  fi
  summary="$(wc -l < "$sums") files match SHA256SUMS"
else
  get shell-manifest.json shell-manifest.json
  get pack/manifest.json pack-manifest.json
  missing=0
  for path in $(jq -r '.files[]' "$tmp/shell-manifest.json") \
              $(jq -r '.files[] | "pack/\(.path).gz"' "$tmp/pack-manifest.json"); do
    if ! curl -fsSI $resolve --max-time 30 -o /dev/null "$origin/$path"; then
      echo "::error::$origin/$path: not found"
      missing=$((missing + 1))
    fi
  done
  [ "$missing" = 0 ] || fail "$missing manifest file(s) not found"
  summary="all manifest files present (no SHA256SUMS: contents not checked)"
fi

for name in "$host" "www.$host"; do
  curl -fsS $resolve --max-time 30 -o /dev/null "https://$name/" ||
    fail "https://$name/: no response"
  # The box's own certificate, when pinned: each box fetches its own.
  echo | openssl s_client -connect "${backend:-$name}:443" -servername "$name" 2>/dev/null |
    openssl x509 -noout -checkend $((min_days * 86400)) >/dev/null ||
    fail "$name: certificate expires within $min_days days; renewal may be failing"
done

echo "OK $origin${backend:+ via $backend}: DNS, headers, $summary, certificates valid $min_days+ days"
