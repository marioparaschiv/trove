use std::{
    collections::BTreeMap,
    env, fs,
    path::PathBuf,
    sync::{LazyLock, mpsc::Receiver},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use trove_core::{
    embedder::{Embedder, Input, LLAMA_CPP_TAG, ModelEvent, vector_version},
    model_setup::{MODEL_FILE, MODEL_REPO, MODEL_REVISION, MODEL_SHA256, ModelFile, hf_hub_cache},
};

const REFERENCE_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/reference/vectors.json");
const WRITE_REFERENCE_ENV: &str = "TROVE_WRITE_REFERENCE";
// Runtimes that agree (llama.cpp CPU and Metal, mlx-vlm) measure at least this; less shows in search results.
const AGREEMENT: f32 = 0.999;

const DIMS: usize = 768;
const EVENT_WAIT: Duration = Duration::from_secs(120);

static EMBEDDER: LazyLock<Embedder> =
    LazyLock::new(|| Embedder::new(model_path(), Duration::from_secs(180)));

/// The pinned model in the Hugging Face cache.
fn model_path() -> PathBuf {
    let fetch = format!("hf download {MODEL_REPO} {MODEL_FILE} --revision {MODEL_REVISION}");
    let hub = hf_hub_cache()
        .unwrap_or_else(|| panic!("no Hugging Face cache folder (no home folder): run `{fetch}`"));
    let path = ModelFile::pinned().path_in_hf_cache(&hub);
    assert!(
        path.is_file(),
        "model not found at {}: run `{fetch}`",
        path.display()
    );

    path
}

fn reference_inputs() -> Vec<(&'static str, Input<'static>)> {
    static LONG_TEXT: LazyLock<String> = LazyLock::new(|| {
        // About 1,800 tokens: past the sliding attention window, so the global layers count as well.
        (1..=60)
            .map(|n| {
                format!(
                    "Note {n}: the review moved to room {} because the projector in the main hall failed again, \
                     and the team agreed to send the slides the night before.",
                    n % 7 + 2
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    });

    vec![
        ("query", Input::Query("payment declined error")),
        ("query-bn", Input::Query("ইলিশ মাছ রান্না")),
        ("query-ar", Input::Query("متى تغلق المكتبة")),
        (
            "document",
            Input::Document {
                title: "Tokyo itinerary",
                text: "Flight to Tokyo Haneda on November 3. Then the train to Kamakura to see the Great Buddha, \
                       and back for dinner in Shibuya.",
            },
        ),
        (
            "document-long",
            Input::Document {
                title: "",
                text: LONG_TEXT.as_str(),
            },
        ),
    ]
}

#[derive(Serialize, Deserialize)]
struct Reference {
    vector_version: String,
    llama_cpp_tag: String,
    vectors: BTreeMap<String, Vec<f32>>,
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn reference_vectors() -> BTreeMap<String, Vec<f32>> {
    let inputs = reference_inputs();
    let made = EMBEDDER
        .embed(&inputs.iter().map(|(_, input)| *input).collect::<Vec<_>>())
        .unwrap();

    inputs
        .iter()
        .map(|(name, _)| name.to_string())
        .zip(made)
        .collect()
}

/// A llama.cpp or model change ships only if this passes, or with a new `vector_version` and a rewritten reference:
/// `TROVE_WRITE_REFERENCE=1 cargo test -p trove-core --test embedder`.
#[test]
fn fixed_texts_match_the_committed_reference_vectors() {
    let made = reference_vectors();
    let version = vector_version(MODEL_SHA256);
    let stored: Option<Reference> = fs::read_to_string(REFERENCE_FILE)
        .ok()
        .map(|json| serde_json::from_str(&json).expect("reference file is valid JSON"));

    if env::var_os(WRITE_REFERENCE_ENV).is_some_and(|value| value == "1") {
        if let Some(stored) = stored
            .as_ref()
            .filter(|stored| stored.vector_version == version)
        {
            for (name, vector) in &made {
                let similarity = cosine(vector, &stored.vectors[name]);
                assert!(
                    similarity >= AGREEMENT,
                    "{name} moved to cosine {similarity} under the same vector_version: change what vector_version covers first"
                );
            }
        }

        let reference = Reference {
            vector_version: version,
            llama_cpp_tag: LLAMA_CPP_TAG.to_owned(),
            vectors: made,
        };
        fs::create_dir_all(PathBuf::from(REFERENCE_FILE).parent().unwrap()).unwrap();
        fs::write(REFERENCE_FILE, serde_json::to_string(&reference).unwrap()).unwrap();
        return;
    }

    let reference = stored.unwrap_or_else(|| {
        panic!("no reference at {REFERENCE_FILE}: write it with {WRITE_REFERENCE_ENV}=1")
    });
    assert_eq!(
        reference.vector_version, version,
        "the reference was made for another vector_version (made with llama.cpp {}): write it again",
        reference.llama_cpp_tag
    );
    assert_eq!(
        made.keys().collect::<Vec<_>>(),
        reference.vectors.keys().collect::<Vec<_>>()
    );
    for (name, vector) in &made {
        let similarity = cosine(vector, &reference.vectors[name]);
        assert!(
            similarity >= AGREEMENT,
            "{name}: cosine {similarity} with the reference made with llama.cpp {}",
            reference.llama_cpp_tag
        );
    }
}

#[test]
fn vectors_are_unit_length() {
    for (name, vector) in reference_vectors() {
        assert_eq!(vector.len(), DIMS, "{name}");
        let norm = cosine(&vector, &vector).sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "{name}: norm {norm}");
    }
}

#[test]
fn a_query_scores_higher_against_its_matching_document_than_an_unrelated_one() {
    let vectors = EMBEDDER
        .embed(&[
            Input::Query("how long to wait between retries when a request fails"),
            Input::Document {
                title: "Networking\nRetry backoff",
                text: "After a failed call, wait 2 seconds, then 4, then 8, doubling up to a minute, and add jitter \
                       so that clients don't all come back at the same moment.",
            },
            Input::Document {
                title: "Recipes\nSourdough",
                text: "Feed the starter the night before, mix flour and water, rest for an hour, then fold the dough \
                       every 30 minutes and bake in a hot Dutch oven.",
            },
        ])
        .unwrap();

    let matching = cosine(&vectors[0], &vectors[1]);
    let unrelated = cosine(&vectors[0], &vectors[2]);
    assert!(
        matching > unrelated,
        "matching {matching} vs unrelated {unrelated}"
    );
}

fn next_event(events: &Receiver<ModelEvent>) -> ModelEvent {
    events
        .recv_timeout(EVENT_WAIT)
        .expect("no model event in time")
}

#[test]
fn the_model_unloads_after_the_idle_timeout_and_the_next_call_loads_it_again() {
    let embedder = Embedder::new(model_path(), Duration::from_millis(200));
    let events = embedder.subscribe();
    assert!(!embedder.is_loaded());

    embedder.embed(&[Input::Query("first")]).unwrap();
    assert_eq!(next_event(&events), ModelEvent::Loaded);
    assert_eq!(next_event(&events), ModelEvent::Unloaded);
    assert!(!embedder.is_loaded());

    let vector = embedder.embed(&[Input::Query("second")]).unwrap();
    assert_eq!(vector.len(), 1);
    assert_eq!(next_event(&events), ModelEvent::Loaded);
}

#[test]
fn unload_on_request_frees_the_model_and_load_brings_it_back() {
    let embedder = Embedder::new(model_path(), Duration::from_secs(180));
    let events = embedder.subscribe();

    embedder.load().unwrap();
    assert_eq!(next_event(&events), ModelEvent::Loaded);
    assert!(embedder.is_loaded());

    embedder.unload();
    assert_eq!(next_event(&events), ModelEvent::Unloaded);
    assert!(!embedder.is_loaded());

    embedder.load().unwrap();
    assert_eq!(next_event(&events), ModelEvent::Loaded);
}

#[test]
fn vector_version_is_stable_and_changes_with_the_model_file() {
    let version = vector_version(MODEL_SHA256);
    assert_eq!(version, vector_version(MODEL_SHA256));
    assert_eq!(version.len(), 64);
    assert!(
        version
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );

    let other_model = "0".repeat(64);
    assert_ne!(version, vector_version(&other_model));
}
