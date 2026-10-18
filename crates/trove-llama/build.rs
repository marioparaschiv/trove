use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const REPOSITORY: &str = "https://github.com/ggml-org/llama.cpp";
const TAG: &str = "b11461";
// A tag can be moved; the commit can't.
const COMMIT: &str = "4d756bc72bf00a4aacf410ae15a2d315f3db400d";
const PATCH: &str = "llama.cpp-no-logits.patch";
// The checkout is sparse: only what the library build reads (the full tree has paths too long for Windows).
const SOURCE_FOLDERS: &[&str] = &["cmake", "ggml", "include", "licenses", "src", "vendor"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={PATCH}");
    println!("cargo:rustc-env=TROVE_LLAMA_CPP_TAG={TAG}");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let patch = manifest_dir.join(PATCH);
    let source = out_dir.join("llama.cpp");

    if !is_ready(&source, &patch) {
        fetch(&out_dir, &source, &patch);
    }

    #[cfg(feature = "generate-bindings")]
    generate_bindings(&source, &manifest_dir.join("src").join("bindings.rs"));

    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS");
    if target_os == "macos" {
        build_static_metal(&source);
    } else {
        build_shared_with_runtime_backends(&source, &out_dir);
    }
}

/// A checkout from an earlier run is reused only if it is still the pinned commit with the current patch applied.
fn is_ready(source: &Path, patch: &Path) -> bool {
    source.join(".git").is_dir()
        && git_output(source, &["rev-parse", "HEAD"]).as_deref() == Some(COMMIT)
        && git_succeeds(source, &["apply", "--reverse", "--check", path_arg(patch)])
}

fn fetch(out_dir: &Path, source: &Path, patch: &Path) {
    let partial = out_dir.join("llama.cpp.partial");
    for folder in [source, partial.as_path()] {
        if folder.exists() {
            fs::remove_dir_all(folder)
                .unwrap_or_else(|error| panic!("failed to remove {}: {error}", folder.display()));
        }
    }

    // core.autocrlf=false keeps LF line endings on Windows, which the patch needs.
    run(Command::new("git").current_dir(out_dir).args([
        "-c",
        "advice.detachedHead=false",
        "clone",
        "--quiet",
        "--depth=1",
        "--filter=blob:none",
        "--sparse",
        "--config=core.autocrlf=false",
        &format!("--branch={TAG}"),
        REPOSITORY,
        path_arg(&partial),
    ]));
    run(Command::new("git")
        .current_dir(&partial)
        .args(["sparse-checkout", "set"])
        .args(SOURCE_FOLDERS));

    let head = git_output(&partial, &["rev-parse", "HEAD"]);
    if head.as_deref() != Some(COMMIT) {
        panic!(
            "llama.cpp tag {TAG} is at {head:?}, expected commit {COMMIT}: refusing to build it"
        );
    }

    run(Command::new("git")
        .current_dir(&partial)
        .args(["apply", path_arg(patch)]));
    fs::rename(&partial, source).unwrap_or_else(|error| {
        panic!("failed to move the llama.cpp checkout into place: {error}")
    });
}

fn base_config(source: &Path) -> cmake::Config {
    let mut config = cmake::Config::new(source);
    // A debug llama.cpp is too slow to embed with, and MSVC's debug runtime would clash with Rust's.
    config
        .profile("Release")
        .define("GGML_NATIVE", "OFF")
        // ggml's own worker threads start from the thread that embeds, so a background embedding's lowered priority
        // reaches them. OpenMP's threads are its own and keep whatever priority they were last given.
        .define("GGML_OPENMP", "OFF")
        .define("LLAMA_BUILD_COMMON", "OFF")
        .define("LLAMA_BUILD_TESTS", "OFF")
        .define("LLAMA_BUILD_TOOLS", "OFF")
        .define("LLAMA_BUILD_EXAMPLES", "OFF")
        .define("LLAMA_BUILD_SERVER", "OFF")
        .define("LLAMA_BUILD_APP", "OFF")
        .define("LLAMA_BUILD_UI", "OFF")
        .define("LLAMA_OPENSSL", "OFF");

    // With the Visual Studio generator the cmake crate replaces the release flags with cc's, which have no
    // optimisation, so CMake's own release flags are put back.
    if env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|target_env| target_env == "msvc") {
        for language in ["C", "CXX"] {
            config.define(
                format!("CMAKE_{language}_FLAGS_RELEASE"),
                "/MD /O2 /Ob2 /DNDEBUG",
            );
        }
    }

    config
}

fn build_static_metal(source: &Path) {
    let install = base_config(source)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("GGML_METAL", "ON")
        .define("GGML_METAL_EMBED_LIBRARY", "ON")
        .define("CMAKE_OSX_DEPLOYMENT_TARGET", "14.0")
        .build();

    println!(
        "cargo:rustc-link-search=native={}",
        install.join("lib").display()
    );
    for library in [
        "llama",
        "ggml",
        "ggml-base",
        "ggml-cpu",
        "ggml-metal",
        "ggml-blas",
    ] {
        println!("cargo:rustc-link-lib=static={library}");
    }
    for framework in ["Foundation", "Metal", "MetalKit", "Accelerate"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=dylib=c++");
}

/// Shared libraries with every CPU variant and Vulkan as backend modules loaded at run time, so one build runs on any
/// CPU, and on the GPU where a Vulkan driver is installed.
fn build_shared_with_runtime_backends(source: &Path, out_dir: &Path) {
    require_vulkan_sdk();

    let install = base_config(source)
        .define("BUILD_SHARED_LIBS", "ON")
        .define("GGML_BACKEND_DL", "ON")
        .define("GGML_CPU_ALL_VARIANTS", "ON")
        .define("GGML_VULKAN", "ON")
        // `llama` needs `ggml` beside it; a binary's RUNPATH doesn't apply to its libraries' own dependencies.
        .define("CMAKE_INSTALL_RPATH", "$ORIGIN")
        .build();

    println!(
        "cargo:rustc-link-search=native={}",
        install.join("lib").display()
    );
    for library in ["llama", "ggml", "ggml-base"] {
        println!("cargo:rustc-link-lib=dylib={library}");
    }

    // ggml loads its backends from the executable's folder, and Windows finds DLLs there too, so the runtime
    // libraries go beside the binaries (`target/<profile>`) and the test executables (`target/<profile>/deps`).
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is <target>/<profile>/build/<package>/out");
    let runtime_libraries = runtime_libraries(&install);
    for destination in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        for library in &runtime_libraries {
            let name = library.file_name().expect("library file name");
            fs::copy(library, destination.join(name)).unwrap_or_else(|error| {
                panic!(
                    "failed to copy {} to {}: {error}",
                    library.display(),
                    destination.display()
                )
            });
        }
    }
}

/// Fails with one message naming what to install, instead of the CMake error deep in ggml's Vulkan build.
fn require_vulkan_sdk() {
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");
    let sdk = env::var_os("VULKAN_SDK").map(PathBuf::from);
    let mut include_dirs: Vec<PathBuf> = sdk.iter().map(|sdk| sdk.join("include")).collect();
    include_dirs.extend(["/usr/include", "/usr/local/include"].map(PathBuf::from));
    let has_header = |header: &str| include_dirs.iter().any(|dir| dir.join(header).is_file());

    // CMake looks in the SDK first, then on PATH.
    let glslc = sdk
        .iter()
        .map(|sdk| sdk.join("bin").join("glslc"))
        .find(|glslc| glslc.with_extension(env::consts::EXE_EXTENSION).is_file())
        .unwrap_or_else(|| PathBuf::from("glslc"));
    let has_glslc = Command::new(&glslc)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());

    let missing: Vec<&str> = [
        (has_header("vulkan/vulkan.h"), "the Vulkan headers"),
        (has_header("spirv/unified1/spirv.hpp"), "SPIRV-Headers"),
        (has_glslc, "glslc"),
    ]
    .into_iter()
    .filter_map(|(found, name)| (!found).then_some(name))
    .collect();

    assert!(
        missing.is_empty(),
        "building llama.cpp's Vulkan backend needs the Vulkan SDK with glslc, but {} were not found. Install the \
         Vulkan SDK from https://vulkan.lunarg.com/sdk/home and set VULKAN_SDK to it (on Debian or Ubuntu, \
         `sudo apt-get install libvulkan-dev spirv-headers glslc` instead).",
        missing.join(", ")
    );
}

fn runtime_libraries(install: &Path) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for folder in [install.join("bin"), install.join("lib")] {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };

        for entry in entries {
            let path = entry
                .unwrap_or_else(|error| panic!("failed to list {}: {error}", folder.display()))
                .path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if name.ends_with(".dll") || name.contains(".so") {
                libraries.push(path);
            }
        }
    }

    assert!(
        !libraries.is_empty(),
        "the llama.cpp build installed no shared libraries in {}",
        install.display()
    );
    libraries
}

// Only what src/lib.rs calls: the full API drags in platform types (such as `FILE`) that would tie the committed
// bindings to the OS they were generated on.
#[cfg(feature = "generate-bindings")]
const BOUND_FUNCTIONS: &[&str] = &[
    "ggml_backend_dev_count",
    "ggml_backend_dev_description",
    "ggml_backend_dev_get",
    "ggml_backend_dev_type",
    "ggml_backend_load_all",
    "ggml_backend_reg_by_name",
    "ggml_backend_reg_get_proc_address",
    "ggml_threadpool_params_default",
    "llama_backend_init",
    "llama_attach_threadpool",
    "llama_batch_free",
    "llama_batch_init",
    "llama_context_default_params",
    "llama_detach_threadpool",
    "llama_encode",
    "llama_free",
    "llama_get_embeddings_seq",
    "llama_init_from_model",
    "llama_log_set",
    "llama_model_default_params",
    "llama_model_free",
    "llama_model_get_vocab",
    "llama_model_load_from_file",
    "llama_model_n_embd_out",
    "llama_set_n_threads",
    "llama_tokenize",
    "llama_vocab_eos",
];

#[cfg(feature = "generate-bindings")]
fn generate_bindings(source: &Path, destination: &Path) {
    bindgen::Builder::default()
        .header(path_arg(&source.join("include").join("llama.h")))
        .clang_arg(format!(
            "-I{}",
            source.join("ggml").join("include").display()
        ))
        .allowlist_function(BOUND_FUNCTIONS.join("|"))
        .prepend_enum_name(false)
        .rust_edition(bindgen::RustEdition::Edition2024)
        .generate()
        .expect("failed to generate llama.cpp bindings")
        .write_to_file(destination)
        .expect("failed to write llama.cpp bindings");
}

fn path_arg(path: &Path) -> &str {
    path.to_str()
        .unwrap_or_else(|| panic!("path is not valid UTF-8: {}", path.display()))
}

fn run(command: &mut Command) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to run {command:?}: {error}"));
    assert!(status.success(), "{command:?} failed with {status}");
}

fn git_output(folder: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(folder)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_succeeds(folder: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(folder)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}
