#!/usr/bin/env bash
#
# Offline secret scan for tracked files.
#
# Used by .github/workflows/security.yml and by the optional pre-commit hook in
# .githooks/pre-commit.
#
# Design notes
#  * It reports the RULE and the LOCATION only. The matched text is never
#    printed, so this script cannot copy a real credential into CI logs or a
#    terminal scrollback — which is the failure mode that turns a secret scanner
#    into a secret leak.
#  * It is dependency-free (POSIX-ish grep only), so the gate adds no
#    supply-chain surface of its own.
#  * It scans `git ls-files`, so build output, node_modules and untracked
#    scratch files are excluded by construction rather than by pattern.
#  * A line may opt out with the marker `scan-secrets:allow` (for documentation
#    and fixtures that legitimately quote a pattern).
#
# Exit status: 0 when clean, 1 when a rule matched, 2 on a usage/setup error.

set -u

cd "$(dirname "$0")/.." || exit 2

if ! command -v git >/dev/null 2>&1; then
  echo "scan-secrets: git is required" >&2
  exit 2
fi

# The scanner itself necessarily contains every pattern, so it must never be
# scanned. Lockfiles are excluded because they hold only integrity hashes and
# registry URLs — no credential material — and they are large.
mapfile -d '' FILES < <(
  git ls-files -z -- \
    ':!scripts/scan-secrets.sh' \
    ':!package-lock.json' \
    ':!src-tauri/Cargo.lock' 2>/dev/null
)

if [ "${#FILES[@]}" -eq 0 ]; then
  echo "scan-secrets: no tracked files to scan"
  exit 0
fi

# rule name | grep flags | ERE pattern
# Patterns use an explicit leading/trailing non-token class instead of \b so
# they behave the same under GNU and BSD grep.
RULES=(
  "AWS access key id||(^|[^A-Za-z0-9])(AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}([^A-Za-z0-9]|$)"
  "AWS secret access key||aws_secret_access_key[^A-Za-z0-9]{0,4}[:=][^A-Za-z0-9]{0,4}[\"'][A-Za-z0-9/+=]{40}[\"']"
  "GitHub token||(^|[^A-Za-z0-9])(gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})"
  "Google API key||(^|[^A-Za-z0-9])AIza[0-9A-Za-z_-]{35}([^0-9A-Za-z_-]|$)"
  "Slack token||(^|[^A-Za-z0-9])xox[abprs]-[0-9A-Za-z-]{10,}"
  "Stripe live key||(^|[^A-Za-z0-9])(sk|rk)_live_[0-9A-Za-z]{16,}"
  "OpenAI-style key||(^|[^A-Za-z0-9])sk-[A-Za-z0-9]{20,}"
  "JWT||(^|[^A-Za-z0-9_-])eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"
  "Private key block|F|-----BEGIN [A-Z ]*PRIVATE KEY-----"
  "Credentials in URL||[a-zA-Z][a-zA-Z0-9+.-]*://[^/[:space:]:@]+:[^/[:space:]:@]+@"
  "Generic secret assignment|i|(api[_-]?key|secret|token|passwd|password)[^A-Za-z0-9]{0,4}[:=][^A-Za-z0-9]{0,4}[\"'][A-Za-z0-9/+_-]{20,}[\"']"
)

# Only regular files still present on disk. An empty list must be handled
# explicitly: `grep` with no file operand reads stdin, which would hang CI.
PRESENT=()
for f in "${FILES[@]}"; do
  [ -f "$f" ] && PRESENT+=("$f")
done

if [ "${#PRESENT[@]}" -eq 0 ]; then
  echo "scan-secrets: no tracked files present on disk"
  exit 0
fi

found=0
report=""

for entry in "${RULES[@]}"; do
  name=${entry%%|*}
  rest=${entry#*|}
  flags=${rest%%|*}
  pattern=${rest#*|}

  # -E extended regex, -n line numbers, -I skip binary files. -H forces the
  # filename prefix even for a single file: without it grep prints just
  # `line:text`, and the parser below would report the matched text instead of
  # the location — the exact leak this script exists to avoid.
  # The optional per-rule flag is appended as its own argv entry (never as an
  # empty string, which grep would read as a file operand).
  grep_args=(-EnIH)
  if [ -n "$flags" ]; then
    grep_args+=("-$flags")
  fi
  matches=$(grep "${grep_args[@]}" -e "$pattern" -- "${PRESENT[@]}" 2>/dev/null || true)
  [ -n "$matches" ] || continue

  while IFS= read -r line; do
    case "$line" in
      *"scan-secrets:allow"*) continue ;;
    esac
    location=${line%%:*}
    lineno=$(printf '%s' "$line" | cut -d: -f2)
    report="${report}${name}|${location}:${lineno}\n"
    found=1
  done <<< "$matches"
done

if [ "$found" -eq 0 ]; then
  echo "scan-secrets: clean (${#PRESENT[@]} tracked files, ${#RULES[@]} rules)"
  exit 0
fi

echo "scan-secrets: possible credential material found"
echo
printf '%b' "$report" | sort -u | while IFS='|' read -r name location; do
  printf '  %-28s %s\n' "$name" "$location"
done
cat <<'EOF'

Matched text is deliberately not shown. Open each location, confirm whether it
is real credential material, and rotate the secret if it was ever committed
(rotating matters more than removing the file: once pushed, assume it is
public). If the hit is a documentation example or a test fixture, append
`scan-secrets:allow` to that line.
EOF
exit 1
