//! Embeddings with llama.cpp, set up like the Python mem (llama-cpp-python `Llama(embedding=True, n_ctx=2048,
//! n_batch=2048, flash_attn=False)` + `embed()`): BOS added, special tokens not parsed, at most 2048 tokens,
//! pooled per sequence by the model's own pooling, then L2-normalized here.
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::num::NonZeroU32;

pub struct Embedder {
    model: &'static LlamaModel,
    ctx: LlamaContext<'static>,
}

impl Embedder {
    pub fn load(path: &str) -> Result<Embedder, String> {
        let t0 = std::time::Instant::now();
        let timing = std::env::var_os("MEM_TIMING").is_some();
        llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(false));
        let backend: &'static LlamaBackend =
            Box::leak(Box::new(LlamaBackend::init().map_err(|e| format!("llama backend: {e}"))?));
        let model: &'static LlamaModel = Box::leak(Box::new(
            LlamaModel::load_from_file(backend, path, &LlamaModelParams::default())
                .map_err(|e| format!("loading {path}: {e}"))?,
        ));
        if timing {
            eprintln!("timing: model load {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);
        }
        let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4);
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(2048))
            .with_n_batch(2048)
            .with_n_ubatch(2048)
            .with_embeddings(true)
            .with_n_threads(threads)
            .with_n_threads_batch(threads)
            .with_flash_attention_policy(0 as llama_cpp_sys_2::llama_flash_attn_type); // DISABLED, as flash_attn=False in Python
        let ctx = model.new_context(backend, params).map_err(|e| format!("llama context: {e}"))?;
        if timing {
            eprintln!("timing: model + context {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);
        }
        Ok(Embedder { model, ctx })
    }

    /// unit-length float32 vector, little-endian bytes (Python: struct.pack(f"{n}f", ...))
    pub fn embed(&mut self, text: &str) -> Result<Vec<u8>, String> {
        let t0 = std::time::Instant::now();
        let mut toks = self.model.vocab().tokenize(text.as_bytes(), true, false);
        toks.truncate(2048);
        self.ctx.clear_kv_cache();
        let mut batch = LlamaBatch::new(toks.len().max(1), 1);
        batch.add_sequence(&toks, 0, true).map_err(|e| format!("batch: {e}"))?;
        self.ctx.decode(&mut batch).map_err(|e| format!("decode: {e}"))?;
        let v: Vec<f32> = self.ctx.embeddings_seq_ith(0).map_err(|e| format!("embeddings: {e}"))?.to_vec();
        self.ctx.clear_kv_cache();
        if std::env::var_os("MEM_TIMING").is_some() {
            eprintln!("timing: embed {} tokens {:.0} ms", toks.len(), t0.elapsed().as_secs_f64() * 1000.0);
        }
        let n = v.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
        let n = if n == 0.0 { 1.0 } else { n };
        let mut out = Vec::with_capacity(v.len() * 4);
        for x in v {
            out.extend_from_slice(&(((x as f64) / n) as f32).to_le_bytes());
        }
        Ok(out)
    }
}
