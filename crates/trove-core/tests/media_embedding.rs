use std::{path::PathBuf, sync::LazyLock, time::Duration};

use trove_core::{
    embedder::{Embedder, EmbedderError, Input, Media, ModelEvent, Profile},
    model_setup::{MMPROJ_FILE, MODEL_FILE, MODEL_REPO, MODEL_REVISION, ModelFile, hf_hub_cache},
};

const IMAGE: &[u8] = include_bytes!("media/red-circle-blue-square.png");
/// Five seconds of synthesised speech: "The weather today will be sunny and warm, with a light breeze from the sea and
/// no rain until the weekend."
const AUDIO: &[u8] = include_bytes!("media/weather-forecast.mp3");
const IMAGE_QUERY: &str = "a red circle next to a blue square";
const AUDIO_QUERY: &str = "a forecast of sunny, warm weather";

const DIMS: usize = 768;
const EVENT_WAIT: Duration = Duration::from_secs(120);

static EMBEDDER: LazyLock<Embedder> = LazyLock::new(|| embedder(Duration::from_secs(180)));

/// A pinned file in the Hugging Face cache.
fn cached(file: ModelFile) -> PathBuf {
    let fetch =
        format!("hf download {MODEL_REPO} {MODEL_FILE} {MMPROJ_FILE} --revision {MODEL_REVISION}");
    let hub = hf_hub_cache()
        .unwrap_or_else(|| panic!("no Hugging Face cache folder (no home folder): run `{fetch}`"));
    let path = file.path_in_hf_cache(&hub);
    assert!(
        path.is_file(),
        "{} not found at {}: run `{fetch}`",
        file.file,
        path.display()
    );

    path
}

fn embedder(idle_timeout: Duration) -> Embedder {
    Embedder::new(cached(ModelFile::pinned()), idle_timeout)
        .with_projector(cached(ModelFile::pinned_projector()))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn query(text: &str) -> Vec<f32> {
    EMBEDDER.embed_query(text).unwrap().to_vec()
}

#[test]
fn an_image_and_a_clip_each_score_highest_against_the_text_that_describes_them() {
    let media = EMBEDDER
        .embed_media(
            &[Media::Image(IMAGE), Media::Audio(AUDIO)],
            Profile::Foreground,
        )
        .unwrap();
    let (image, audio) = (&media[0], &media[1]);
    let (image_query, audio_query) = (query(IMAGE_QUERY), query(AUDIO_QUERY));

    let image_scores = (cosine(image, &image_query), cosine(image, &audio_query));
    assert!(
        image_scores.0 > image_scores.1,
        "image: matching {} vs other {}",
        image_scores.0,
        image_scores.1
    );
    let audio_scores = (cosine(audio, &audio_query), cosine(audio, &image_query));
    assert!(
        audio_scores.0 > audio_scores.1,
        "audio: matching {} vs other {}",
        audio_scores.0,
        audio_scores.1
    );
}

#[test]
fn media_vectors_are_unit_length_in_the_text_space() {
    let text = EMBEDDER
        .embed(&[Input::Query(IMAGE_QUERY)], Profile::Foreground)
        .unwrap();
    let media = EMBEDDER
        .embed_media(
            &[Media::Image(IMAGE), Media::Audio(AUDIO)],
            Profile::Background,
        )
        .unwrap();

    for vector in media.iter().chain(&text) {
        assert_eq!(vector.len(), DIMS);
        let norm = cosine(vector, vector).sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "norm {norm}");
    }
}

#[test]
fn decoded_samples_embed_as_the_file_they_came_from() {
    let decoded = EMBEDDER.decode_audio(AUDIO).unwrap();
    assert_eq!(decoded.sample_rate, 16_000);
    let seconds = decoded.samples.len() as f32 / decoded.sample_rate as f32;
    assert!((5.0..6.0).contains(&seconds), "{seconds} s");

    let vectors = EMBEDDER
        .embed_media(
            &[Media::Audio(AUDIO), Media::AudioSamples(&decoded.samples)],
            Profile::Foreground,
        )
        .unwrap();
    assert_eq!(vectors[0], vectors[1]);
}

#[test]
fn a_file_of_the_wrong_kind_is_refused() {
    for media in [
        Media::Image(AUDIO),
        Media::Audio(IMAGE),
        Media::Image(b"not an image"),
    ] {
        let error = EMBEDDER
            .embed_media(&[media], Profile::Foreground)
            .unwrap_err();
        assert!(
            matches!(error, EmbedderError::Model(_)),
            "{media:?}: {error:?}"
        );
    }
}

#[test]
fn without_a_projector_media_is_refused_and_text_still_embeds() {
    let embedder = Embedder::new(cached(ModelFile::pinned()), Duration::from_secs(180));

    let error = embedder
        .embed_media(&[Media::Image(IMAGE)], Profile::Foreground)
        .unwrap_err();
    assert!(matches!(error, EmbedderError::NoProjector), "{error:?}");
    assert_eq!(
        embedder
            .embed(&[Input::Query("text")], Profile::Foreground)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn the_projector_unloads_with_the_model_and_the_next_media_call_loads_both_again() {
    let embedder = embedder(Duration::from_millis(200));
    let events = embedder.subscribe();

    let first = embedder
        .embed_media(&[Media::Image(IMAGE)], Profile::Foreground)
        .unwrap();
    assert_eq!(events.recv_timeout(EVENT_WAIT).unwrap(), ModelEvent::Loaded);
    assert_eq!(
        events.recv_timeout(EVENT_WAIT).unwrap(),
        ModelEvent::Unloaded
    );

    let again = embedder
        .embed_media(&[Media::Image(IMAGE)], Profile::Foreground)
        .unwrap();
    assert_eq!(events.recv_timeout(EVENT_WAIT).unwrap(), ModelEvent::Loaded);
    assert_eq!(again, first);
}
