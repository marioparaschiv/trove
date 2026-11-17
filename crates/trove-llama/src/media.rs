//! Images and audio through libmtmd: the model's projector encodes them into rows the text model takes in place of
//! token embeddings, so they land in the same vector space as text.

use std::{ffi::CString, path::Path, ptr::NonNull};

use crate::{LlamaError, bindings::*, normalised};

/// One image or audio clip to embed.
#[derive(Debug, Clone, Copy)]
pub enum Media<'a> {
    /// An encoded image file: JPEG, PNG, BMP, GIF and the other formats stb_image reads.
    Image(&'a [u8]),
    /// An encoded audio file: WAV, MP3 or FLAC.
    Audio(&'a [u8]),
    /// Mono PCM samples at the projector's sample rate, as `decode_audio` returns them.
    AudioSamples(&'a [f32]),
}

/// A loaded projector (mmproj) for one text model, freed on drop. It must be dropped before that model.
pub(crate) struct Projector {
    context: NonNull<mtmd_context>,
}

impl Projector {
    /// Loads the projector at `path` for `model`, on `gpu` if there is one, else the CPU.
    pub(crate) fn load(
        path: &Path,
        model: NonNull<llama_model>,
        gpu: Option<ggml_backend_dev_t>,
        threads: i32,
    ) -> Result<Self, LlamaError> {
        let c_path = path
            .to_str()
            .and_then(|path| CString::new(path).ok())
            .ok_or_else(|| LlamaError::InvalidPath(path.to_path_buf()))?;

        // SAFETY: plain value struct from mtmd's defaults.
        let mut params = unsafe { mtmd_context_params_default() };
        // Without a device, mtmd would take any GPU, integrated ones too, which `first_gpu` skips.
        params.use_gpu = gpu.is_some();
        params.device = gpu.unwrap_or(std::ptr::null_mut());
        params.n_threads = threads;
        params.print_timings = false;
        params.warmup = false;

        // SAFETY: `c_path` is a valid NUL-terminated string and `model` is a live model, which outlives this value.
        let context =
            NonNull::new(unsafe { mtmd_init_from_file(c_path.as_ptr(), model.as_ptr(), params) })
                .ok_or_else(|| LlamaError::LoadProjector(path.to_path_buf()))?;
        let projector = Self { context };

        // Positions are plain sequence positions below; M-RoPE models need 2D ones.
        // SAFETY: the context is live.
        if unsafe { mtmd_decode_use_mrope(projector.context.as_ptr()) } {
            return Err(LlamaError::LoadProjector(path.to_path_buf()));
        }

        Ok(projector)
    }

    /// The sample rate audio is embedded at, or `None` if the projector has no audio encoder.
    pub(crate) fn audio_sample_rate(&self) -> Option<u32> {
        // SAFETY: the context is live.
        let rate = unsafe { mtmd_get_audio_sample_rate(self.context.as_ptr()) };
        u32::try_from(rate).ok()
    }

    /// Decodes a WAV, MP3 or FLAC file to mono samples at `audio_sample_rate`.
    pub(crate) fn decode_audio(&self, bytes: &[u8]) -> Result<Vec<f32>, LlamaError> {
        let bitmap = self.decode(bytes, true, 0)?;
        // SAFETY: the bitmap is live and holds `n_bytes` bytes of f32 samples.
        let samples = unsafe {
            let data = mtmd_bitmap_get_data(bitmap.0.as_ptr());
            let count = mtmd_bitmap_get_n_bytes(bitmap.0.as_ptr()) / size_of::<f32>();
            std::slice::from_raw_parts(data.cast::<f32>(), count).to_vec()
        };
        Ok(samples)
    }

    /// Embeds one media input with the text model's `context`, whose batch holds at most `max_tokens`. `index`
    /// numbers it in errors.
    pub(crate) fn embed(
        &mut self,
        model: NonNull<llama_model>,
        context: NonNull<llama_context>,
        max_tokens: usize,
        dims: usize,
        media: Media<'_>,
        index: usize,
    ) -> Result<Vec<f32>, LlamaError> {
        let bitmap = match media {
            Media::Image(bytes) => self.decode(bytes, false, index)?,
            Media::Audio(bytes) => self.decode(bytes, true, index)?,
            Media::AudioSamples(samples) => {
                if self.audio_sample_rate().is_none() {
                    return Err(LlamaError::UnsupportedMedia(index));
                }
                // SAFETY: `samples` is valid for its length; mtmd copies it.
                let bitmap =
                    unsafe { mtmd_bitmap_init_from_audio(samples.len(), samples.as_ptr()) };
                Bitmap(NonNull::new(bitmap).ok_or(LlamaError::UnreadableMedia(index))?)
            }
        };

        let chunks = self.tokenize(&bitmap, index)?;
        let total: usize = chunks
            .iter()
            .map(|chunk| {
                // SAFETY: the chunk belongs to the live chunk list.
                unsafe { mtmd_input_chunk_get_n_tokens(chunk) }
            })
            .sum();
        if total > max_tokens {
            return Err(LlamaError::InputTooLong(index));
        }

        // SAFETY: `model` is live.
        let n_embd = unsafe { llama_model_n_embd_inp(model.as_ptr()) } as usize;
        let batch = Batch::new(context)?;
        let mut position: llama_pos = 0;
        for chunk in chunks.iter() {
            // SAFETY: the chunk belongs to the live chunk list.
            let kind = unsafe { mtmd_input_chunk_get_type(chunk) };
            if kind == MTMD_INPUT_CHUNK_TYPE_TEXT {
                let mut count = 0;
                // SAFETY: a text chunk's tokens are `count` tokens it owns.
                let tokens = unsafe {
                    let tokens = mtmd_input_chunk_get_tokens_text(chunk, &mut count);
                    std::slice::from_raw_parts(tokens, count)
                };
                for &token in tokens {
                    // SAFETY: the batch is live.
                    let at = unsafe { llama_batch_ext_add_token(batch.0.as_ptr(), 0, token) };
                    batch.place(at, &mut position, index)?;
                }
                continue;
            }

            // SAFETY: both contexts are live; a media chunk's output is `n_tokens` rows of `n_embd` floats.
            let rows = unsafe {
                if mtmd_encode_chunk(self.context.as_ptr(), chunk) != 0 {
                    return Err(LlamaError::EncodeMedia(index));
                }
                let rows = mtmd_get_output_embd(self.context.as_ptr());
                let count = mtmd_input_chunk_get_n_tokens(chunk);
                std::slice::from_raw_parts(rows, count * n_embd)
            };
            for row in rows.chunks_exact(n_embd) {
                let row = llama_embd {
                    data: row.as_ptr(),
                    n_rows: 1,
                    n_embd,
                };
                // SAFETY: the batch is live and copies the row.
                let at = unsafe { llama_batch_ext_add_embd(batch.0.as_ptr(), 0, row) };
                batch.place(at, &mut position, index)?;
            }
        }

        // One mixed batch of text tokens and media rows, encoded in one go like a text: the model pools them together.
        // SAFETY: the context and the batch are live.
        let status = unsafe {
            llama_process(
                context.as_ptr(),
                LLAMA_PROCESS_TYPE_ENCODE,
                batch.0.as_ptr(),
            )
        };
        if status != 0 {
            return Err(LlamaError::Encode(status));
        }

        // SAFETY: the context is live; a non-null result points at `dims` floats owned by the context, copied out
        // before the next call into it.
        unsafe {
            let pooled = llama_get_embeddings_seq(context.as_ptr(), 0);
            if pooled.is_null() {
                return Err(LlamaError::MissingEmbedding(index));
            }
            Ok(normalised(std::slice::from_raw_parts(pooled, dims)))
        }
    }

    /// Decodes an image or audio file, failing unless it is audio exactly when `audio` is set.
    fn decode(&self, bytes: &[u8], audio: bool, index: usize) -> Result<Bitmap, LlamaError> {
        // SAFETY: `bytes` is valid for its length; mtmd decodes it into a bitmap of its own.
        let wrapper = unsafe {
            mtmd_helper_bitmap_init_from_buf(
                self.context.as_ptr(),
                bytes.as_ptr(),
                bytes.len(),
                false,
                mtmd_helper_init_opt_default(),
            )
        };
        let bitmap =
            Bitmap(NonNull::new(wrapper.bitmap).ok_or(LlamaError::UnreadableMedia(index))?);

        // SAFETY: the bitmap is live.
        if unsafe { mtmd_bitmap_is_audio(bitmap.0.as_ptr()) } != audio {
            return Err(LlamaError::UnreadableMedia(index));
        }
        Ok(bitmap)
    }

    /// The media marker alone, so the input is the media between the model's begin and end tokens, framed by its
    /// special tokens like a text. The model card passes media without a task prompt.
    fn tokenize(&self, bitmap: &Bitmap, index: usize) -> Result<Chunks, LlamaError> {
        // SAFETY: mtmd's marker is a static NUL-terminated string.
        let marker = unsafe { std::ffi::CStr::from_ptr(mtmd_default_marker()) };
        let text = mtmd_input_text {
            text: marker.as_ptr(),
            text_len: marker.to_bytes().len(),
            add_special: true,
            parse_special: true,
        };

        // SAFETY: plain allocation, freed by `Chunks`.
        let chunks = Chunks(
            NonNull::new(unsafe { mtmd_input_chunks_init() })
                .ok_or(LlamaError::EncodeMedia(index))?,
        );
        let bitmaps = [bitmap.0.as_ptr().cast_const()];
        // SAFETY: every pointer is live for the call, and the one bitmap matches the one marker.
        let status = unsafe {
            mtmd_tokenize(
                self.context.as_ptr(),
                chunks.0.as_ptr(),
                &text,
                bitmaps.as_ptr(),
                bitmaps.len(),
            )
        };
        if status != 0 {
            return Err(LlamaError::UnreadableMedia(index));
        }

        Ok(chunks)
    }
}

impl Drop for Projector {
    fn drop(&mut self) {
        // SAFETY: created in `load` and freed only here.
        unsafe { mtmd_free(self.context.as_ptr()) };
    }
}

struct Bitmap(NonNull<mtmd_bitmap>);

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: owned by this value.
        unsafe { mtmd_bitmap_free(self.0.as_ptr()) };
    }
}

struct Chunks(NonNull<mtmd_input_chunks>);

impl Chunks {
    fn iter(&self) -> impl Iterator<Item = *const mtmd_input_chunk> + '_ {
        // SAFETY: the list is live and every index below its size names a chunk.
        let count = unsafe { mtmd_input_chunks_size(self.0.as_ptr()) };
        (0..count).map(|index| unsafe { mtmd_input_chunks_get(self.0.as_ptr(), index) })
    }
}

impl Drop for Chunks {
    fn drop(&mut self) {
        // SAFETY: owned by this value; frees its chunks too.
        unsafe { mtmd_input_chunks_free(self.0.as_ptr()) };
    }
}

/// llama.cpp's extended batch, which mixes token ids and embedding rows.
struct Batch(NonNull<llama_batch_ext>);

impl Batch {
    fn new(context: NonNull<llama_context>) -> Result<Self, LlamaError> {
        // SAFETY: the context is live; the batch takes its sizes from it.
        NonNull::new(unsafe { llama_batch_ext_init(context.as_ptr()) })
            .map(Self)
            .ok_or(LlamaError::CreateContext)
    }

    /// Gives the entry just added at `at` the next position in sequence 0 and asks for its output, which mean
    /// pooling reads.
    fn place(&self, at: i32, position: &mut llama_pos, index: usize) -> Result<(), LlamaError> {
        // SAFETY: the batch is live; `at` is checked before it is used.
        let placed = at >= 0
            && unsafe {
                llama_batch_ext_set_pos(self.0.as_ptr(), at, position)
                    && llama_batch_ext_set_output_embd(self.0.as_ptr(), at, true)
            };
        if !placed {
            return Err(LlamaError::InputTooLong(index));
        }

        *position += 1;
        Ok(())
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        // SAFETY: owned by this value.
        unsafe { llama_batch_ext_free(self.0.as_ptr()) };
    }
}
