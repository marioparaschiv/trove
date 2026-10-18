# /orchestrate: Trove v1 (resume)

You are the orchestrator for Trove v1, resuming a run started on another machine. Trove is a local, cross-platform Rust service that indexes an Obsidian vault, embeds it with EmbeddingGemma 2 through an in-process llama.cpp, builds a knowledge graph (links plus semantic edges), and serves search, related notes and categories over localhost HTTP to an Obsidian plugin and a Raycast extension.

Run the remaining tickets with parallel subagents, one git worktree per slice (`isolation: "worktree"`, `subagent_type: general-purpose`), gating each slice on its blockers. Never build on an unverified slice.

## Repo and branches
- Repo: `~/trove-rust` (clone of private `github.com/marioparaschiv/trove-rust`). Not `~/trove`, which is an unrelated project.
- Integration branch: `trove-v1`. Check it out in the main checkout; every slice worktree branches from the current `trove-v1`.
- Spec: `.scratch/trove/spec.md`. Tickets: `.scratch/trove/issues/NN-*.md` (each finished ticket has a `## Comments` section with contracts later slices need). Handoffs for in-flight slices: `.scratch/trove/handoffs/s07.md` to `s11.md`.
- `main` holds only the scaffold commit. Nothing merges into `main` unless the user asks.
- Push rules: you may push `trove-v1` and `slice/*` branches to `origin` (the user approved pushing so they can follow along). No tags, no GitHub releases, no force-push.
- GitHub's default branch is currently `slice/05-ci` (it was the first branch pushed). Leave it; tell the user in the final report.

## Done (merged into trove-v1, verified)
| Slice | Ticket | Notes |
|---|---|---|
| S1 | 01-core-contracts | `trove-core` lib + `trove` bin; schema v1; config; indexer processors; search `Stage`/`Pipeline`; CLI skeleton |
| S2 | 02-model-setup | `trove setup`, HF cache lookup, resumable download, idle timeout |
| S3 | 03-embedder | `crates/trove-llama` (llama.cpp b11461 fetched and built by `build.rs`, DigUp no-logits patch, committed bindings); embedder with lazy load/idle unload; `vector_version` |
| S4 | 04-service-lifecycle | `trove serve`, runtime file, token auth, idle exit, `/events`; Bun workspace at `clients/` with `clients/ts-client` (generated from `trove openapi`), Biome |
| S5 | 05-ci | `.github/workflows/ci.yml`, green on Windows/macOS/Linux with caches; Metal vs CPU check |
| S6 | 06-tracer-meaning-search (+ 06b llama log noise) | indexing, meaning search via CLI and HTTP, fixture vault `crates/trove/tests/fixtures/vault`, helpers `crates/trove/tests/common/mod.rs` |

## Remaining slices
| Slice | Tickets | Blocked by | State |
|---|---|---|---|
| S7 | 07-hybrid-search-eval | S6 | WIP on `slice/07-hybrid-search-eval`, see `handoffs/s07.md` |
| S8 | 08-change-detection | S6 | not started, plan in `handoffs/s08.md` |
| S9 | 09-graph-rerank | S6 | not started, design in `handoffs/s09.md` |
| S10 | 10-obsidian-quick-switcher | S6 | scaffold only on `slice/10-obsidian-quick-switcher`, see `handoffs/s10.md` |
| S11 | 11-raycast-search | S6 | WIP on `slice/11-raycast-search`, see `handoffs/s11.md` |
| S12 | 12-sync-store | S8 | |
| S13 | 13-related-notes + 18-releases-first-run (both edit the plugin) | S9, S10, S5 | |
| S14 | 14-categories-core | S9 | |
| S15 | 15-categories-obsidian + 16-suggestions-orphans (both edit plugin and core) | S14, S10, S12, S9 | |
| S16 | 17-category-edits | S15 | |

Frontier now: S7, S8, S9, S10, S11 (dispatch all five in one message). Each resumes from its handoff and its existing branch (`git checkout slice/NN-...` inside the worktree; if the branch has no commits of its own, branch fresh from `trove-v1`).

Known overlaps to reconcile at merge:
- S7 owns the per-result `why` (`Signal` enum, `Candidate.why: BTreeSet<Signal>`); S9 adds `Signal::Graph`. If S7 merges first, tell S9 to use it directly.
- S7 and S9 both add flags to `crates/trove/src/commands/search.rs`.
- S8 and S9 both touch indexing: S8 does change detection in the scan (caller of `Indexer`), S9 appends a link-parser processor in `Indexer::standard`. Keep the processor interface intact.
- S10 and S11 both add Bun workspace packages; regenerate `clients/bun.lock` with `bun install` on conflicts.
- Cargo.toml/Cargo.lock conflicts: keep both sides' deps (alphabetical in `[workspace.dependencies]`), regenerate the lock with `cargo build`.

## Decisions already made (don't relitigate)
- Chunks stay keyed by a hash of their text only (spec "rename reuses all vectors"), even though the preamble chunk's prompt includes the note title.
- Meaning-match threshold is not pinned in the spec; S7 tunes it with its eval (currently `MIN_Z = 2.5`; `trove search "how to keep my bread culture alive"` wrongly returns nothing on the fixture vault).
- No in-memory vector cache until measured (S6 reads vectors from SQLite per query).
- llama.cpp logs: only errors are printed.
- Every dot-folder in a vault is skipped (covers `.obsidian`, `.trash`, `.trove`).

## Machine setup (do this first, before dispatching)
This server is Linux x86_64, 32 cores. Verify and fix in this order:
1. Rust 1.97.1 via rustup (`rustup toolchain install 1.97.1 --component clippy rustfmt`), CMake (`sudo apt-get install -y cmake`), a C/C++ compiler (gcc/g++ present). Bun and Node 24 are present. `gh` is authenticated.
2. Model: the tests need the real model in the Hugging Face cache at `~/.cache/huggingface/hub/models--ggml-org--embeddinggemma-2-GGUF/snapshots/bfcd298762cc34d0357ece5ebdd31791a3a374d8/embeddinggemma-2-Q8_0.gguf` (size 309,855,456, SHA-256 `2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135`; pin also in `crates/trove-core/src/model_setup.rs`). If missing, fetch with `curl -L -C -` from `https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/bfcd298762cc34d0357ece5ebdd31791a3a374d8/embeddinggemma-2-Q8_0.gguf` and verify.
3. DigUp reference (MIT) shallow clone at `/tmp/digup-src/repo` (`git clone --depth 1 https://github.com/ARahim3/DigUp /tmp/digup-src/repo`).
4. Baseline on `trove-v1`: `cargo build`, `cargo clippy --all-targets` (no warnings), `cargo test`, and in `clients/`: `bun install`, `bun run typecheck`, `bun run lint`, `bun run test`, `bun run build`. All were green on Windows at `trove-v1`; if Linux fails, fix that first as its own small commit on `trove-v1`.
   Known Linux failure (observed on this server at `trove-v1` 1673216): Rust build, clippy and all cargo tests pass, but `bun run test` in `clients/` fails with `trove serve exited with 127: /home/dev/trove-rust/target/debug/trove: error while loading shared libraries: libllama.so.0: cannot open shared object file: No such file or directory`. `cargo test` passes because cargo sets the library path; a binary started directly does not find the llama/ggml shared libraries (ticket 03 and 05 Comments note the missing `$ORIGIN` rpath on Linux). Fix this first, in `crates/trove-llama` (build script), so the `trove` binary runs on its own on Linux; verify with `target/debug/trove status` from a plain shell, then rerun the full baseline, commit on `trove-v1`, push.

## Subagent prompt (one per slice)
Each prompt contains, filled in:
1. Tickets: the ticket file paths and, for S7 to S11, the handoff path. Read them, then `.scratch/trove/spec.md` once, and the `## Comments` of every ticket marked done. The spec holds every pinned value; don't change it. Touch only what the tickets need; never work on other tickets. New work goes in new files with one-line registrations in shared files.
2. Machine: Linux server, repo at `~/trove-rust`, tools as above. Write LF line endings.
3. Model: in the HF cache path above; don't download it. Tests must not read or create the real Trove data folder; use `TROVE_DATA_DIR` temp folders and `crates/trove/tests/common/`.
4. Reference code: DigUp at `/tmp/digup-src/repo`; read only what the ticket needs.
5. Conventions: deps declared once in root `[workspace.dependencies]`, used with `dep.workspace = true`; member crates use `version.workspace = true` and `[lints] workspace = true`; only `trove-llama` overrides `unsafe_code`. TS packages under `clients/` (Bun workspace, Biome, scripts `typecheck`/`lint`/`test`/`build`). Follow the user's global rules: no type escape hatches, validate external input, no swallowed errors, no speculative features, tests use the real model and real SQLite and replace only true externals, no sleeps (inject timeouts, wait on events), comments only for non-obvious why, no em-dashes. `cargo fmt --all`.
6. Testing seams: seam 1 is the `trove` binary's HTTP API and CLI against the fixture vault in a temp data folder; seam 2 is the embedder (reference vectors, backend tolerance). Don't test internals.
7. Done means: every acceptance criterion met; `cargo build`, `cargo clippy --all-targets` (no warnings), `cargo test` pass; touched TS packages pass all four scripts. Commit on the slice branch with short imperative messages, no AI attribution. Append `## Comments` to each ticket with what later tickets need, set `Status: done`.
8. Report: per ticket, criteria met or not, commands and results, anything unverified, deviations with reasons.

Slice-specific:
- S10, S13, S15: there is no Obsidian on this server. Verify by build, lint, typecheck, tests; list manual checks for the user.
- S11: no Raycast here; same.
- S13 (ticket 18): release assets must build, but don't push a tag or create a release. Verify the release workflow with `act` if available, else a dry run of the build steps.
- S9: record the Leiden crate finding in ticket 09's `## Comments` for S14.

## Procedure
1. Machine setup and baseline (above).
2. Dispatch the frontier in one message.
3. When a slice returns, rerun its checks yourself in its worktree. Never trust the report alone. On failure, SendMessage the same agent with the output.
4. `git merge --no-ff` the verified slice into `trove-v1`, resolve conflicts, rerun full workspace checks on `trove-v1`, push `trove-v1`, recompute the frontier.
5. Stop and ask the user if a ticket is impossible as written, a pinned spec value is wrong, a slice needs a decision the spec doesn't make, or two attempts at a slice fail.
6. End-to-end checks on `trove-v1`: after S7, `trove eval` on the fixture queries prints recall@5, recall@10, MRR; after S12, delete the local index, rebuild, and `/status` shows zero model calls.
7. When all slices are merged: full suite across every crate and TS package; push `trove-v1` and confirm CI is green (`gh run watch`).
8. Run `/sreview` on `trove-v1` as the final step.
9. Report: per-ticket outcome, manual checks left (Obsidian visuals, Raycast, release tag), review verdict, what was pushed.

## Suggested skills
- `/sreview` (final review, required by step 8).
- `tdd` for slices adding behaviour behind seam 1.
- `systematic-debugging` when a slice's tests fail twice or the watcher tests flake.
- `pause-orchestration` if context runs low.
