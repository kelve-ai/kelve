#!/bin/sh
# Installs the `kelve` command, the newest release, for macOS or Linux:
#
#   curl -fsSL https://raw.githubusercontent.com/kelve-ai/kelve/main/install.sh | sh
#
# It downloads the build for this computer from GitHub Releases, checks it
# against the release's checksums, and puts it in ~/.local/bin, or in
# KELVE_INSTALL_DIR. Then it sets kelve up: connects your agent, adds its
# hook for your notes and lessons, and how much kelve marks for you.
set -eu

repo=kelve-ai/kelve
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) name=macos-arm64 ;;
  Darwin-x86_64) name=macos-x86_64 ;;
  Linux-x86_64) name=linux-x86_64 ;;
  Linux-aarch64 | Linux-arm64) name=linux-arm64 ;;
  *)
    echo "kelve has no build for $(uname -s) on $(uname -m) yet." >&2
    exit 1
    ;;
esac
dir=${KELVE_INSTALL_DIR:-$HOME/.local/bin}
# KELVE_RELEASES installs from somewhere else, such as a release served
# while developing kelve (bun dev/kelve.ts newcomer).
base=${KELVE_RELEASES:-"https://github.com/$repo/releases/latest/download"}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL "$base/kelve-$name.tar.gz" -o "$tmp/kelve-$name.tar.gz"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  check="sha256sum -c -"
else
  check="shasum -a 256 -c -"
fi
if ! (cd "$tmp" && grep " kelve-$name.tar.gz\$" SHA256SUMS | $check >/dev/null); then
  echo "The download doesn't match the release's checksum, so it wasn't installed. Try again." >&2
  exit 1
fi
tar -xzf "$tmp/kelve-$name.tar.gz" -C "$tmp"
mkdir -p "$dir"
mv "$tmp/kelve" "$dir/kelve"
chmod +x "$dir/kelve"

echo "kelve is installed in $dir."
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Add $dir to your PATH, so your agent and your terminal find kelve." ;;
esac

# Set it up: connect the agent, add its hook, and how much kelve marks. A
# person in a terminal is asked; an agent running this is left one word to
# choose.
if [ -t 1 ] && (exec </dev/tty) 2>/dev/null; then
  "$dir/kelve" setup </dev/tty || true
else
  "$dir/kelve" setup </dev/null || true
fi
