Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

The embedder (spec "Embedding runtime", test seam 2). A crate that builds llama.cpp from the pinned tag and commit with DigUp's no-logits patch, with committed bindgen bindings. DigUp (MIT, https://github.com/ARahim3/DigUp) has the patch and build flags in `scripts/llama.cpp-no-logits.patch` and `scripts/build-llama.sh`, and its llama.h usage in `Sources/LlamaRuntime/LlamaEmbedder.swift`. The core gets an embedder that loads lazily, embeds batches as queries or documents with the spec's prompts (mean-pooled, L2-normalised), and unloads after the model idle timeout or on request.

Tests find the model through the data folder or the Hugging Face cache (pre-fetch with `hf download` at the pinned revision). libclang is at `C:\Users\eternal\scoop\apps\llvm\current\bin` (set `LIBCLANG_PATH`). Confirm the document prompt against the model card and update the spec if it differs.

## Acceptance criteria

- [x] Builds on this Windows machine (MSVC, CPU backends loaded at run time)
- [x] Reference vector test: fixed texts match committed vectors within tolerance
- [x] A query scores higher against its matching document than against an unrelated one
- [x] Vectors are unit length
- [x] After the idle timeout (injected, not slept) the model is unloaded; the next call loads it again
- [x] `vector_version` is exposed and changes when prompts or pinned files change

## Blocked by

- 01-core-contracts

## Comments

Layout.
- `crates/trove-llama` (lib `trove_llama`): the only crate with `unsafe` (crate-level `allow(unsafe_code)`). `build.rs` fetches, patches and builds llama.cpp; `src/bindings.rs` is committed bindgen output; `src/lib.rs` is the safe wrapper (`EmbeddingModel::load(path, max_tokens)`, `embed(&mut self, &[&str]) -> Vec<Vec<f32>>`, `LLAMA_CPP_TAG`). `llama.cpp-no-logits.patch` is DigUp's (MIT).
- `crates/trove-core/src/embedder.rs`: `Embedder::new(model_path, idle_timeout)`, `load()` (prewarm), `embed(&[Input]) -> Vec<Vec<f32>>` (one 768-dim unit vector per input, in order; loads on first call), `unload()`, `is_loaded()`, `subscribe() -> mpsc::Receiver<ModelEvent>` (`Loaded`/`Unloaded`, for status and SSE). `Input::Query(text)` / `Input::Document { title, text }` (an empty title becomes `none`). `Embedder` is `Send + Sync`, not `Clone`: share it behind an `Arc`. Calls block and are serialised by a mutex; async callers use `spawn_blocking`.
- `vector_version(model_sha256) -> String` (64 lowercase hex): SHA-256 over the model SHA-256, `LLAMA_CPP_TAG`, the prompt pieces and `Chunker::VERSION` (new const in `indexer/chunker.rs`; bump it when chunk texts change). Pass the effective model's SHA-256 (`config.model.sha256` from 02). Not written to `meta` yet: 06 writes it when it starts storing vectors. The embedder tests find the model through 02's `ModelFile::pinned()`: the data folder (`path_in`) first, then the HF cache (`model_setup::hf_hub_cache()` and `ModelFile::path_in_hf_cache`, made public for this).
- Inputs over `CONTEXT_TOKENS` (2048) are cut keeping the end token, as transformers does; 06 should still split sections to fit. There is no token count API yet; 06 adds it to `EmbeddingModel` (it holds the vocab).

llama.cpp build.
- `build.rs` runs `git clone --depth=1 --filter=blob:none --sparse --branch=b11461` into `OUT_DIR`, checks out only `cmake ggml include licenses src vendor` (the full tree has paths too long for Windows), refuses anything but commit `4d756bc7...`, applies the patch, then builds with the `cmake` crate (Release, `GGML_NATIVE=OFF`, tools/tests/server off). A checkout is reused while it is still that commit with the patch applied. Needs `git`, CMake and network on the first build of each target dir.
- Windows and Linux: shared libraries, `GGML_BACKEND_DL=ON`, `GGML_CPU_ALL_VARIANTS=ON`. The runtime libraries (`llama`, `ggml`, `ggml-base`, every `ggml-cpu-*` variant) are copied into `target/<profile>` and `target/<profile>/deps`, where ggml looks for backends (the executable's folder). Release packaging (05) must ship them beside `trove`; on Linux the binary also needs an `$ORIGIN` rpath (cargo's `LD_LIBRARY_PATH` covers `cargo run`/`cargo test` only). On Windows the DLLs need the VC++ runtime (`msvcp140`, `vcruntime140`, and `vcomp140` for OpenMP).
- macOS: static libraries with Metal (`GGML_METAL_EMBED_LIBRARY=ON`, deployment target 14.0), linked with Foundation, Metal, MetalKit, Accelerate and libc++. Not built yet; first verified by 05's CI.
- MSVC: under the Visual Studio generator the cmake crate replaces `CMAKE_<LANG>_FLAGS_RELEASE` with flags that have no optimisation; `build.rs` puts `/O2` back (without it the embedder tests took 108 s instead of 6 s).
- Build env: nothing beyond `git`, CMake and a C/C++ toolchain (MSVC on Windows; the cmake crate picks the Visual Studio generator, `CMAKE_GENERATOR=Ninja` also works). Linux arm64 would need GCC 14+ or Clang for the armv9.2 CPU variants (see DigUp's `build-llama.sh`). libclang (`LIBCLANG_PATH`) only to regenerate bindings: `cargo build -p trove-llama --features generate-bindings`. The bindings allowlist only the functions `lib.rs` calls, which keeps platform types such as `FILE` out; enums come out as `c_int` (generated on Windows), which is ABI-compatible with the `c_uint` GCC/Clang would give.
- A clean llama.cpp build takes about 1 minute here; each new target dir or profile builds it again (debug, release and the `generate-bindings` feature each get their own).

Reference vectors: `crates/trove-core/tests/reference/vectors.json`, written with `TROVE_WRITE_REFERENCE=1 cargo test -p trove-core --test embedder` (refuses to overwrite with drifted vectors under the same `vector_version`). The texts are DigUp's; these Windows CPU vectors agree with DigUp's committed Metal vectors at cosine 0.9998 or higher on all five, so CPU and Metal vectors mix at this tag.

Measured (Ryzen 7 5800X, 8 threads, release): load 0.7 s; 128 documents of ~200 tokens in 32 s (~800 tokens/s); a short query in 16 ms. The embedder tests take about 7 s.
