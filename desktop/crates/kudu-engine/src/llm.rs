//! A small local language model (GGUF, llama.cpp) for smart formatting and Command mode.
//! Optional: only used when the user downloads one. Runs on the CPU, greedy decoding.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

fn backend() -> Result<&'static LlamaBackend> {
    if let Some(backend) = BACKEND.get() {
        return Ok(backend);
    }
    // llama.cpp can be initialised only once per process: two models loading at the same time
    // must not both try.
    static INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = INIT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(backend) = BACKEND.get() {
        return Ok(backend);
    }
    let mut backend = LlamaBackend::init().map_err(|e| anyhow!("llama.cpp: {e}"))?;
    backend.void_logs();
    Ok(BACKEND.get_or_init(|| backend))
}

pub struct SmallModel {
    model: LlamaModel,
    threads: i32,
}

impl SmallModel {
    /// Loads a GGUF model. A second or two for the 0.5B model.
    pub fn load(path: &Path) -> Result<Self> {
        let start = Instant::now();
        let params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(backend()?, path, &params)
            .with_context(|| format!("Couldn't load {}", path.display()))?;
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let threads = cores.saturating_sub(2).clamp(2, 8) as i32;
        log::info!("Language model ready in {} ms", start.elapsed().as_millis());
        Ok(Self { model, threads })
    }

    /// Runs one chat turn (system instructions + user message) and returns the reply.
    /// Stops at the end of the reply, after `max_tokens`, or when `timeout` passes (then
    /// returns an error: a partial reply is never used).
    pub fn chat(&self, system: &str, user: &str, max_tokens: usize, timeout: Duration) -> Result<String> {
        let start = Instant::now();
        // Qwen's chat format (ChatML).
        let prompt = format!(
            "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
        );
        let vocab = self.model.vocab();
        // No BOS token (the chat format starts with its own markers); parse the markers.
        let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
        let n_ctx = (tokens.len() + max_tokens + 16).next_power_of_two().max(1024) as u32;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(n_ctx)
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads);
        let mut ctx = self
            .model
            .new_context(backend()?, ctx_params)
            .map_err(|e| anyhow!("context: {e}"))?;

        let mut batch = LlamaBatch::new(n_ctx as usize, 1);
        let last = tokens.len() as i32 - 1;
        for (i, token) in tokens.iter().enumerate() {
            batch.add(*token, i as i32, &[0], i as i32 == last)?;
        }
        ctx.decode(&mut batch).map_err(|e| anyhow!("decode: {e}"))?;

        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let first_position = batch.n_tokens();
        for position in (first_position..).take(max_tokens) {
            if start.elapsed() > timeout {
                return Err(anyhow!("timed out after {} ms", start.elapsed().as_millis()));
            }
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if vocab.is_eog(token) {
                break;
            }
            let bytes = vocab.token_to_piece(token, false, None);
            let mut piece = String::with_capacity(32);
            let _ = decoder.decode_to_string(&bytes, &mut piece, false);
            output.push_str(&piece);
            if output.contains("<|im_end|>") {
                break;
            }
            batch.clear();
            batch.add(token, position, &[0], true)?;
            ctx.decode(&mut batch).map_err(|e| anyhow!("decode: {e}"))?;
        }
        let output = output.replace("<|im_end|>", "");
        log::info!("Language model replied in {} ms", start.elapsed().as_millis());
        Ok(output.trim().to_string())
    }
}
