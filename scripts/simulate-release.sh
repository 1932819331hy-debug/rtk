#!/usr/bin/env bash
# Run the release pipeline locally, for one target, before tagging.
#
# The steps are read out of .github/workflows/release.yml rather than copied,
# so this cannot drift from what CI runs. `${{ matrix.* }}` is filled in for the
# host; everything else executes verbatim. What it cannot do is cross-compile
# the other four targets, publish, or push to the tap -- so it proves the
# packaging and naming logic, not the upload.
#
# Usage: bash scripts/simulate-release.sh [--skip-build]
set -uo pipefail

cd "$(git rev-parse --show-toplevel)"
WF=.github/workflows/release.yml
SKIP_BUILD=0
[ "${1:-}" = "--skip-build" ] && SKIP_BUILD=1

TARGET=$(rustc -vV | awk '/^host:/ {print $2}')
case "$TARGET" in *windows*) ARCHIVE=zip ;; *) ARCHIVE=tar.gz ;; esac
SIM=target/release-sim
rm -rf "$SIM"; mkdir -p "$SIM"

PASS=0; FAIL=0; SKIP=0
ok()   { printf '  \033[32mPASS\033[0m  %s\n' "$1"; PASS=$((PASS+1)); }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; FAIL=$((FAIL+1)); }
skip() { printf '  \033[33mSKIP\033[0m  %s\n' "$1"; SKIP=$((SKIP+1)); }
head_() { printf '\n\033[1m%s\033[0m\n' "$1"; }

# sha256sum is coreutils; macOS ships shasum. The workflow runs on ubuntu.
if ! command -v sha256sum >/dev/null; then
  sha256sum() { shasum -a 256 "$@"; }
  export -f sha256sum 2>/dev/null || true
fi

# Pull one step's `run:` block out of the workflow and fill in the matrix.
step() { # job, step-name-substring
  python3 - "$WF" "$1" "$2" "$TARGET" "$ARCHIVE" <<'PY'
import sys, yaml
wf, job, want, target, archive = sys.argv[1:6]
d = yaml.safe_load(open(wf))
for s in d["jobs"][job]["steps"]:
    if want in s.get("name", "") and "run" in s:
        r = s["run"]
        r = r.replace("${{ matrix.target }}", target).replace("${{ matrix.archive }}", archive)
        # release.yml guards Windows/Unix packaging with `if:`; we run the Unix arm.
        print(r)
        break
PY
}

run_step() { # job, name, label
  local body; body=$(step "$1" "$2")
  if [ -z "$body" ]; then bad "$3 — step not found in $WF"; return 1; fi
  if bash -c "$body" >"$SIM/$3.log" 2>&1; then ok "$3"; else
    bad "$3 (see $SIM/$3.log)"; tail -4 "$SIM/$3.log" | sed 's/^/        /'; return 1
  fi
}

head_ "Target"
echo "  host target : $TARGET"
echo "  archive     : $ARCHIVE"
echo "  note        : CI builds 5 targets; this simulates 1."

# ── build job ────────────────────────────────────────────────────────────────
head_ "Job: build"
if [ "$SKIP_BUILD" = 1 ]; then
  skip "cargo builds (--skip-build); reusing target/$TARGET/release"
else
  run_step build "Build rtk"                    "build-rtk"
  run_step build "Build rtk (Apache-2.0 edition)" "build-rtk-oss"
  run_step build "Build rtk-proxy"              "build-rtk-proxy"
fi

run_step build "Package (Unix)"        "package-rtk"
run_step build "Package rtk-oss (Unix)"   "package-rtk-oss"
run_step build "Package rtk-proxy (Unix)" "package-rtk-proxy"

# ── deb / rpm ────────────────────────────────────────────────────────────────
head_ "Jobs: build-deb / build-deb-oss / build-rpm / build-rpm-oss"
if command -v cargo-deb >/dev/null; then
  run_step build-deb     "Build DEB" "deb-full"
  run_step build-deb-oss "Build DEB" "deb-oss"
else
  skip "deb (cargo-deb not installed: cargo install cargo-deb)"
fi
if command -v cargo-generate-rpm >/dev/null; then
  run_step build-rpm     "Generate RPM" "rpm-full"
  run_step build-rpm-oss "Generate RPM" "rpm-oss"
else
  skip "rpm (cargo-generate-rpm not installed)"
fi

# ── release job ──────────────────────────────────────────────────────────────
head_ "Job: release"
mkdir -p "$SIM/artifacts"
for f in rtk-"$TARGET.$ARCHIVE" rtk-oss-"$TARGET.$ARCHIVE" rtk-proxy-"$TARGET.$ARCHIVE"; do
  [ -f "$f" ] && cp "$f" "$SIM/artifacts/"
done
find target/debian target/generate-rpm -type f \( -name '*.deb' -o -name '*.rpm' \) \
  -exec cp {} "$SIM/artifacts/" \; 2>/dev/null

( cd "$SIM" && mkdir -p release &&
  find artifacts -type f \( -name "*.tar.gz" -o -name "*.zip" -o -name "*.deb" -o -name "*.rpm" \) \
    -exec cp {} release/ \; ) && ok "flatten artifacts"

# the real step, verbatim
naming=$(step release "version-agnostic")
( cd "$SIM" && bash -c "$naming" ) && ok "version-agnostic names" || bad "version-agnostic names"

( cd "$SIM/release" && sha256sum ./* > checksums.txt 2>/dev/null ) && ok "checksums" || bad "checksums"

# ── homebrew job: the checksum lookups, verbatim ─────────────────────────────
head_ "Job: homebrew (checksum lookups)"
GITHUB_OUTPUT="$SIM/gh_output"; : > "$GITHUB_OUTPUT"; export GITHUB_OUTPUT
sha_block=$(python3 - "$WF" <<'PY'
import sys, yaml
d = yaml.safe_load(open(sys.argv[1]))
for s in d["jobs"]["homebrew"]["steps"]:
    if "Parse checksums" in s.get("name", ""):
        print(s["run"]); break
PY
)
( cd "$SIM/release" && bash -c "$sha_block" ) 2>/dev/null
# Only the host target can resolve here; the rest are legitimately empty.
resolved=0
while IFS='=' read -r k v; do
  [ -z "${v:-}" ] && continue
  resolved=$((resolved+1))
  # a lookup must never resolve to another edition's archive
  hit=$(grep -c "$v" "$SIM/release/checksums.txt" 2>/dev/null || echo 0)
  file=$(awk -v s="$v" '$1==s {print $2}' "$SIM/release/checksums.txt" | head -1)
  case "$k:$file" in
    oss_*:*rtk-oss-*)  ok "$k -> $(basename "$file")" ;;
    oss_*:*)           bad "$k -> $(basename "$file") (expected an rtk-oss archive)" ;;
    *:*rtk-oss-*)      bad "$k -> $(basename "$file") (leaked the Apache-2.0 archive)" ;;
    *:*rtk-proxy-*)    bad "$k -> $(basename "$file") (leaked the daemon archive)" ;;
    *:*)               ok "$k -> $(basename "$file")" ;;
  esac
done < "$GITHUB_OUTPUT"
[ "$resolved" -gt 0 ] || skip "no checksum lookup matched this host target (expected off-linux)"

# ── assertions on the artifacts themselves ───────────────────────────────────
head_ "Artifact contents"
R="$SIM/release"
assert_tar() { # archive, must-contain..., must-not-contain
  local a="$1"; shift
  [ -f "$R/$a" ] || { bad "$a missing"; return; }
  local listing; listing=$(tar -tzf "$R/$a")
  local missing=""
  for want in $1; do echo "$listing" | grep -qx "$want" || missing="$missing $want"; done
  shift
  local leaked=""
  for no in ${1:-}; do echo "$listing" | grep -qx "$no" && leaked="$leaked $no"; done
  if [ -n "$missing" ]; then bad "$a missing:$missing"
  elif [ -n "$leaked" ]; then bad "$a must not contain:$leaked"
  else ok "$a = $(echo "$listing" | tr '\n' ' ')"; fi
}
assert_tar "rtk-$TARGET.$ARCHIVE"       "rtk LICENSE LICENSE-ENGINE" ""
assert_tar "rtk-oss-$TARGET.$ARCHIVE"   "rtk LICENSE"                "LICENSE-ENGINE"
assert_tar "rtk-proxy-$TARGET.$ARCHIVE" "rtk-proxy LICENSE LICENSE-ENGINE" ""

head_ "Binary identity"
b="$SIM/x"; rm -rf "$b"; mkdir -p "$b"
tar -xzf "$R/rtk-oss-$TARGET.$ARCHIVE" -C "$b" 2>/dev/null
if [ -x "$b/rtk" ]; then
  ok "Apache-2.0 archive installs a binary named 'rtk' (hook commands keep working)"
  "$b/rtk" pipeline show >/dev/null 2>&1 \
    && bad "oss binary still has 'rtk pipeline'" \
    || ok "oss binary has no 'rtk pipeline' subcommand"
else bad "no 'rtk' inside the Apache-2.0 archive"; fi

head_ "Result"
printf "  %d passed, %d failed, %d skipped\n" "$PASS" "$FAIL" "$SKIP"
echo "  artifacts: $SIM/release"
[ "$FAIL" -eq 0 ]
