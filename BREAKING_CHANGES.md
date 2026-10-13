# Breaking changes

What RTK v1.0 breaks relative to the current 0.x release.

The 0.x release has no engine, no proxy daemon and no `pipeline.toml`, so
everything v1.0 adds in those areas is new. The list is therefore short: **one renamed command, one reported number that moves, one build-profile
change, one licence change — and the licence one has an opt-out (§4) — and one
removed flag (§5).**

If you use RTK today and write no `pipeline.toml`, §1 and §2 are the items that
touch you — and neither stops anything working. §5 does, but only if you pass
`--ultra-compact`.

---

## 1. `rtk proxy <cmd>` is now `rtk bypass <cmd>`

The meta-command that runs a single command unfiltered was renamed, so it is not
confused with `rtk llm-proxy` (the daemon that intercepts provider API traffic).

This is the only command removed from the CLI surface. Everything else v1.0 adds
is additive (`pipeline`, `methods`, `tasks`, `llm-proxy`, `bypass`, and the
`gain` flags below).

**It still works.** `rtk proxy` is a hidden alias of `rtk bypass` with identical
behaviour and no deprecation notice.

```diff
-rtk proxy git log -20
+rtk bypass git log -20
```

**Do:** re-run `rtk init` — the awareness text RTK writes into `RTK.md` /
`CLAUDE.md` now says `rtk bypass`, and an existing project keeps the old wording
until you do. Move your own scripts over too. Expect the alias to be removed in
a later release.

---

## 2. `rtk gain` reports a lower reduction on the same database

Outputs whose input exceeded the agent's ~30k-char cap no longer count toward the
headline rate. They are reported separately, as "N oversized outputs excluded".

A few very large outputs (for example a `cat` of a huge file) could previously
dominate the total and inflate the headline. The figure on your existing
database may therefore drop; your data has not changed.

**Do:** nothing, unless you quote or script that percentage. No flag or field was
removed; `-f json` gains an `oversized_excluded` count alongside the rest.

---

## 3. The release binary unwinds, and is larger

`panic = "abort"` is removed from `[profile.release]`, so a panicking filter
fails open (your command's output passes through unchanged) instead of aborting
the tool call. The release binary is noticeably larger as a result.

**Do:** if you package RTK or ship it somewhere size-constrained, re-check your
budget.

---

## 4. The default `rtk` binary now contains BUSL-1.1 code

| Crate | Licence | |
|---|---|---|
| `rtk` (sources) | Apache-2.0 | unchanged |
| `rtk-text` | Apache-2.0 | new — shared text primitives |
| `rtk-engine` | **BUSL-1.1** | new — pipeline DSL, methods, classifiers |
| `rtk-proxy` | **BUSL-1.1** | new — the daemon; `rtk` never links it |

The default build links `rtk-engine`, so that artifact is Apache-2.0 **and**
BUSL-1.1. Running it is free, commercial use included; reselling it or hosting
it for others needs a licence — contact@rtk-ai.app. Both BUSL crates convert to
Apache-2.0 on **2030-09-09**.

### If you need Apache-2.0 only, there is an edition for that

The engine is an optional dependency, so the same `rtk` binary builds without
it. The binary keeps its name, so a `rtk hook claude` already written into an
agent's settings works either way — and the two Homebrew formulae declare
`conflicts_with`, since only one can own the name.

```bash
curl -fsSL https://raw.githubusercontent.com/rtk-ai/rtk/refs/heads/master/install.sh | RTK_EDITION=oss sh
brew install rtk-ai/tap/rtk-oss
cargo install --git https://github.com/rtk-ai/rtk rtk --no-default-features
```

**Keeps:** the hook rewrite, every command filter, `init`, `gain`, `trust`,
`bypass`, `llm-proxy`, recall.
**Drops:** the pipeline DSL, PostToolUse compression, classifiers, delegation
tasks, and `gain`'s attribution views.

It ships through every channel the default edition does — `rtk-oss-<target>`
archives, `rtk-oss_amd64.deb`, `rtk-oss.x86_64.rpm` — each carrying `LICENSE`
alone, because there is no engine inside to carry `LICENSE-ENGINE` for.

Point a compliance reviewer at a prebuilt path: the artifact contains no BUSL
code and the only licence text travelling with it says so. Installing from
source still clones the BUSL directories — it compiles none of them, but the
files land on disk.

**Do:** nothing, if you install the default edition and run it. Take the
Apache-2.0 edition if your policy keeps non-OSI licences out of the tree.

---

## 5. `--ultra-compact` is removed

The global `--ultra-compact` flag is gone. Only `rtk gh` and `rtk glab` ever
read it — there it swapped word labels for single letters in list headers and
state icons (`[open]` → `O`). Every other command ignored it.

Passing it now fails. Before the subcommand (`rtk --ultra-compact git status`)
rtk no longer recognises it and exits 127 with
`[rtk: No such file or directory (os error 2)]`; after the subcommand
(`rtk git status --ultra-compact`) it is handed to the underlying tool, which
rejects it as an unknown option.

**Do:** drop the flag from any script or alias. `rtk gh` and `rtk glab` output
is the normal (word-label) form, which was already the default.

---

## Not breaking

- **`rtk gain`** — no flag or field removed. Eight added: `--by-method`,
  `--by-rule`, `--by-layer`, `--by-pipeline`, `--pipeline <NAME>`, `--shadow`,
  `--text`, `--watch`. The TUI is redrawn and the data behind it is a superset;
  the one value that changes is the headline rate, see §2.
- **Version numbers** — `rtk`, `rtk-text`, `rtk-engine`, `rtk-plugin` and
  `rtk-proxy` share one version, so the new crates go to 1.0.0 with the CLI and
  a fix in one bumps all five. The Apache-2.0 edition (`rtk-oss`) carries the
  same number. `rtk --version` still reports a single number, in either
  edition.
- **`config.toml`** — no key RTK previously read has been dropped.
  `[hooks] exclude_commands`, `transparent_prefixes` and `[awareness] level` all
  behave as before. Unknown keys are ignored rather than rejected, so an older
  file loads.
- **`filters.toml`** — same grammar, same eight-stage order, same inline tests,
  still consulted by the rewrite. Custom filters are not affected by the engine.
- **The tracking database** — schema 1 → 2 is additive (`CREATE TABLE IF NOT
  EXISTS`, `ALTER TABLE ADD COLUMN`). No rows are dropped on upgrade; your
  savings history survives.
- **Hook registration** — with no `pipeline.toml`, `rtk init` writes exactly one
  `PreToolUse` entry with matcher `Bash`, as today, and `PreToolUse` output is
  byte-identical. The matcher is now derived from the active pipeline instead of
  being a constant, which is why it stays `Bash` until a rule needs more.
