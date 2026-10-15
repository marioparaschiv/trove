Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

The embedder (spec "Embedding runtime", test seam 2). A crate that builds llama.cpp from the pinned tag and commit with DigUp's no-logits patch, with committed bindgen bindings. DigUp (MIT, https://github.com/ARahim3/DigUp) has the patch and build flags in `scripts/llama.cpp-no-logits.patch` and `scripts/build-llama.sh`, and its llama.h usage in `Sources/LlamaRuntime/LlamaEmbedder.swift`. The core gets an embedder that loads lazily, embeds batches as queries or documents with the spec's prompts (mean-pooled, L2-normalised), and unloads after the model idle timeout or on request.

Tests find the model through the data folder or the Hugging Face cache (pre-fetch with `hf download` at the pinned revision). libclang is at `C:\Users\eternal\scoop\apps\llvm\current\bin` (set `LIBCLANG_PATH`). Confirm the document prompt against the model card and update the spec if it differs.

## Acceptance criteria

- [ ] Builds on this Windows machine (MSVC, CPU backends loaded at run time)
- [ ] Reference vector test: fixed texts match committed vectors within tolerance
- [ ] A query scores higher against its matching document than against an unrelated one
- [ ] Vectors are unit length
- [ ] After the idle timeout (injected, not slept) the model is unloaded; the next call loads it again
- [ ] `vector_version` is exposed and changes when prompts or pinned files change

## Blocked by

- 01-core-contracts
