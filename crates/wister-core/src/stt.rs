//! Transcripción local con whisper.cpp (vía `whisper-rs`).

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::audio::{Audio, WHISPER_SAMPLE_RATE};

/// Opciones de cada transcripción.
#[derive(Debug, Clone)]
pub struct Options {
    /// Código de idioma ("es", "en", ...) o "auto" para autodetectar.
    pub language: String,
    /// Texto que orienta a Whisper: nombres propios, jerga, estilo de puntuación.
    pub initial_prompt: Option<String>,
    pub threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            language: "es".into(),
            initial_prompt: None,
            threads: default_threads(),
        }
    }
}

/// Todos los núcleos lógicos hasta 8; más allá whisper.cpp casi no escala.
pub fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4)
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub elapsed: Duration,
}

/// Un modelo cargado en memoria, listo para transcribir.
pub struct Engine {
    // El estado mantiene vivo al contexto (el modelo) con un `Arc` interno.
    state: WhisperState,
    pub load_time: Duration,
    pub gpu: bool,
}

/// Silencia los logs de whisper.cpp y ggml, que por defecto imprimen mucho en stderr.
pub fn silence_native_logs() {
    whisper_rs::install_logging_hooks();
}

/// Si el binario se compiló con algún backend de GPU (`cuda` o `vulkan`).
pub fn gpu_available() -> bool {
    cfg!(any(feature = "cuda", feature = "vulkan"))
}

impl Engine {
    pub fn load(model_path: &Path, use_gpu: bool) -> Result<Self> {
        let start = Instant::now();
        let path = model_path
            .to_str()
            .context("la ruta del modelo no es UTF-8 válido")?;
        let mut params = WhisperContextParameters::default();
        let gpu = use_gpu && gpu_available();
        params.use_gpu(gpu).flash_attn(gpu);
        let ctx = WhisperContext::new_with_params(path, params)
            .with_context(|| format!("no se pudo cargar el modelo {}", model_path.display()))?;
        let state = ctx
            .create_state()
            .context("no se pudo crear el estado de Whisper")?;
        Ok(Self {
            state,
            load_time: start.elapsed(),
            gpu,
        })
    }

    pub fn transcribe(&mut self, audio: &Audio, opts: &Options) -> Result<Transcript> {
        if audio.sample_rate != WHISPER_SAMPLE_RATE {
            bail!(
                "el audio tiene que estar a {WHISPER_SAMPLE_RATE} Hz (vino a {} Hz)",
                audio.sample_rate
            );
        }
        let start = Instant::now();

        // Greedy es bastante más rápido que beam search y para dictado alcanza.
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(&opts.language));
        params.set_n_threads(opts.threads as i32);
        params.set_no_context(true);
        params.set_suppress_nst(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        if let Some(prompt) = &opts.initial_prompt {
            params.set_initial_prompt(prompt);
        }

        // Whisper trabaja con ventanas de 30 s; con menos de 1 s tiende a inventar,
        // así que se completa con silencio hasta llegar a 1 s.
        let min_len = WHISPER_SAMPLE_RATE as usize;
        let samples: std::borrow::Cow<[f32]> = if audio.samples.len() < min_len {
            let mut padded = audio.samples.clone();
            padded.resize(min_len, 0.0);
            padded.into()
        } else {
            (&audio.samples[..]).into()
        };

        self.state
            .full(params, &samples)
            .context("falló la transcripción")?;

        let mut text = String::new();
        for segment in self.state.as_iter() {
            text.push_str(&segment.to_str_lossy()?);
        }
        Ok(Transcript {
            text: text.trim().to_string(),
            elapsed: start.elapsed(),
        })
    }
}
