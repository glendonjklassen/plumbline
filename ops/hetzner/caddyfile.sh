#!/bin/sh
# Prints the Caddyfile a live box runs: the `content:` block of cloud-init.yaml
# with the placeholder `:80` replaced by the four hostnames and `<other box
# IPv4>` by the argument. Both boxes run this file, each naming the other, so
# a header change is edited once (in cloud-init.yaml) and put on each box with
#
#   ops/hetzner/caddyfile.sh <other box IPv4> | ssh deploy@<box> 'sudo tee /etc/caddy/Caddyfile >/dev/null && sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy'
set -eu
other=${1:?usage: caddyfile.sh <other box IPv4>}
dir=$(dirname "$(realpath "$0")")
awk '
  /^  - path: \/etc\/caddy\/Caddyfile$/ { in_file = 1; next }
  in_file && /^    content: \|$/ { in_block = 1; next }
  in_block && /^runcmd:/ { exit }
  in_block { sub(/^      /, ""); print }
' "$dir/cloud-init.yaml" |
  sed -e 's/^:80 {$/plumblinebible.org, www.plumblinebible.org, plumblinebible.ch, www.plumblinebible.ch {/' \
      -e "s/<other box IPv4>/$other/"
