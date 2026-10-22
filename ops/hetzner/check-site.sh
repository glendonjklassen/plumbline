#!/bin/sh
# Checks that the live site serves what the release built. Run by the release
# after every upload, and daily by .github/workflows/site-check.yml, because
# the server changes between releases too: Caddy and Debian update themselves,
# the certificate renews itself, and a compromised box would change files
# without a release.
#
#   check-site.sh https://plumblinebible.org [SHA256SUMS]
#
# - DNS: both names must resolve to exactly EXPECT_ADDRS, so a hijacked record
#   fails here even though the box itself is untouched.
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
# The Hetzner box (ops/hetzner/README.md). A rebuild changes them.
expect_addrs=${EXPECT_ADDRS:-77.42.10.193 2a01:4f9:c015:c326::1}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() { echo "::error::$*"; exit 1; }

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
    curl -fsS --max-time 30 -o "$tmp/$2" "$origin/$1" && return 0
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
  curl -sS --max-time 30 -D "$tmp/headers" -o /dev/null "$origin/$path" ||
    fail "$origin/$path: no response"
  got_csp=$(tr -d '\r' < "$tmp/headers" | sed -n 's/^[Cc]ontent-[Ss]ecurity-[Pp]olicy: //p')
  [ "$got_csp" = "$want_csp" ] ||
    fail "$origin/$path: Content-Security-Policy [$got_csp] does not match cloud-init.yaml [$want_csp]"
  grep -qi '^strict-transport-security: max-age=[1-9]' "$tmp/headers" ||
    fail "$origin/$path: no Strict-Transport-Security header"
  grep -qi '^x-content-type-options: nosniff' "$tmp/headers" ||
    fail "$origin/$path: no X-Content-Type-Options header"
done

if [ -n "$sums" ]; then
  # Every file at once, then one comparison that names what differs. No
  # Accept-Encoding is sent, so Caddy serves the stored bytes.
  mkdir "$tmp/site"
  awk -v o="$origin" '{print "url = \"" o "/" $2 "\"\noutput = \"" $2 "\""}' "$sums" > "$tmp/curl.conf"
  (cd "$tmp/site" && curl -sS --parallel --parallel-max 8 --create-dirs --max-time 120 -K "$tmp/curl.conf") || true
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
    if ! curl -fsSI --max-time 30 -o /dev/null "$origin/$path"; then
      echo "::error::$origin/$path: not found"
      missing=$((missing + 1))
    fi
  done
  [ "$missing" = 0 ] || fail "$missing manifest file(s) not found"
  summary="all manifest files present (no SHA256SUMS: contents not checked)"
fi

for name in "$host" "www.$host"; do
  curl -fsS --max-time 30 -o /dev/null "https://$name/" ||
    fail "https://$name/: no response"
  echo | openssl s_client -connect "$name:443" -servername "$name" 2>/dev/null |
    openssl x509 -noout -checkend $((min_days * 86400)) >/dev/null ||
    fail "$name: certificate expires within $min_days days; renewal may be failing"
done

echo "OK $origin: DNS, headers, $summary, certificates valid $min_days+ days"
