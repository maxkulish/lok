#!/usr/bin/env bash
# Open a PR on maxkulish/homebrew-tap that points Formula/lok.rb at a published
# lok release. Does NOT auto-merge.
#
# Usage: scripts/bump-homebrew-formula.sh vYYYYMMDD.N.P
#
# The sha256 values come from the release's own .sha256 assets, so the release
# must already be published with all four targets. Re-running for a version the
# tap already has, or one with an open bump PR, exits 0 without changes: the
# release workflow calls this, and its re-runs are meant to be idempotent.
#
# Prerequisites: `gh` authenticated with push access to maxkulish/homebrew-tap
# (locally your own login; in CI the HOMEBREW_TAP_TOKEN secret as GH_TOKEN).

set -euo pipefail

TAG="${1:-}"
REPO="maxkulish/lok"
TAP="maxkulish/homebrew-tap"

if ! printf '%s' "$TAG" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "Error: usage: $0 vX.Y.Z (final release tags only, no -rc)" >&2
    exit 1
fi
command -v gh >/dev/null 2>&1 || { echo "Error: gh CLI required" >&2; exit 1; }

VERSION="${TAG#v}"
BRANCH="bump-lok-$TAG"

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

gh release download "$TAG" -R "$REPO" -p '*.tar.gz.sha256' -D "$WORKDIR/sha"

sha_for() {
    local file="$WORKDIR/sha/lok-$TAG-$1.tar.gz.sha256"
    [[ -f "$file" ]] || { echo "Error: release $TAG has no asset $(basename "$file")" >&2; exit 1; }
    awk '{print $1}' "$file"
}

SHA_MAC_ARM="$(sha_for aarch64-apple-darwin)"
SHA_MAC_INTEL="$(sha_for x86_64-apple-darwin)"
SHA_LINUX_ARM="$(sha_for aarch64-unknown-linux-gnu)"
SHA_LINUX_INTEL="$(sha_for x86_64-unknown-linux-gnu)"

if gh pr list -R "$TAP" --head "$BRANCH" --state open --json number --jq '.[].number' | grep -q .; then
    echo "Open PR for $BRANCH already exists on $TAP; nothing to do."
    exit 0
fi

gh repo clone "$TAP" "$WORKDIR/tap" -- --depth=1 --quiet

URL="https://github.com/$REPO/releases/download/$TAG/lok-$TAG"
cat > "$WORKDIR/tap/Formula/lok.rb" <<EOF
class Lok < Formula
  desc "Declarative multi-LLM orchestration across Claude, Codex, Gemini and Ollama"
  homepage "https://github.com/$REPO"
  version "$VERSION"
  license "MIT"

  if OS.mac? && Hardware::CPU.arm?
    url "$URL-aarch64-apple-darwin.tar.gz"
    sha256 "$SHA_MAC_ARM"
  elsif OS.mac? && Hardware::CPU.intel?
    url "$URL-x86_64-apple-darwin.tar.gz"
    sha256 "$SHA_MAC_INTEL"
  elsif OS.linux? && Hardware::CPU.arm?
    url "$URL-aarch64-unknown-linux-gnu.tar.gz"
    sha256 "$SHA_LINUX_ARM"
  elsif OS.linux? && Hardware::CPU.intel?
    url "$URL-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "$SHA_LINUX_INTEL"
  end

  def install
    bin.install "lok", "lokomotiv"
  end

  test do
    system bin/"lok", "--version"
  end
end
EOF

cd "$WORKDIR/tap"
if git diff --quiet -- Formula/lok.rb; then
    echo "Formula/lok.rb is already at $TAG; nothing to do."
    exit 0
fi

git checkout -q -b "$BRANCH"
git add Formula/lok.rb
git -c user.name="${GIT_AUTHOR_NAME:-$(git config user.name || echo lok-release)}" \
    -c user.email="${GIT_AUTHOR_EMAIL:-$(git config user.email || echo lok-release@users.noreply.github.com)}" \
    commit -q -m "Bump lok to $TAG"
git push -q -u origin "$BRANCH"

# --head is required: the shallow clone fetches only main, so gh cannot infer
# the pushed branch.
gh pr create -R "$TAP" --base main --head "$BRANCH" \
    --title "Bump lok to $TAG" \
    --body "Automated bump from the lok release pipeline.

sha256 values come from the \`.sha256\` assets of https://github.com/$REPO/releases/tag/$TAG.

Not auto-merged - review and merge manually."
