//! Transcripción local con whisper.cpp (vía `whisper-rs`).

use std::path::Path;
use std::sync::OnceLock;
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
    /// Achicar la ventana del codificador al largo del audio (ver `audio_ctx_for`).
    /// Es bastante más rápido con audios cortos, pero puede bajar la calidad.
    pub fit_audio_ctx: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            language: "es".into(),
            initial_prompt: None,
            threads: default_threads(),
            fit_audio_ctx: false,
        }
    }
}

/// Frases que Whisper inventa con audio casi vacío: vienen de los subtítulos de
/// videos con los que se entrenó. Solo se descartan cuando son todo el texto.
const FRASES_FANTASMA: &[&str] = &[
    "gracias",
    "muchas gracias",
    "gracias por ver",
    "gracias por ver el video",
    "suscribete",
    "subtitulos por la comunidad de amaraorg",
    "subtitulos realizados por la comunidad de amaraorg",
    "thank you",
    "thanks for watching",
    "thank you for watching",
    "you",
    "obrigado",
    "obrigada",
];

/// Si el texto es solamente una de las frases que Whisper suele inventar con silencio.
pub fn is_hallucination(text: &str) -> bool {
    let normalizado: String = text
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ê' => 'e',
            'í' | 'ì' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            otro => otro,
        })
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let normalizado = normalizado.split_whitespace().collect::<Vec<_>>().join(" ");
    FRASES_FANTASMA.contains(&normalizado.as_str())
}

/// Ventana del codificador (`audio_ctx`) para un audio de `samples` muestras a 16 kHz.
///
/// Whisper siempre codifica 30 s (1500 posiciones, 50 por segundo) aunque el audio dure
/// 3: con un audio corto, casi todo el trabajo es sobre silencio. Se deja un margen y se
/// redondea a múltiplos de 64 para que no cambie el tamaño en cada dictado.
pub fn audio_ctx_for(samples: usize) -> i32 {
    const COMPLETA: usize = 1500;
    const MINIMA: usize = 128;
    const MARGEN: usize = 64;
    let posiciones = samples.div_ceil(WHISPER_SAMPLE_RATE as usize / 50);
    ((posiciones + MARGEN).next_multiple_of(64)).clamp(MINIMA, COMPLETA) as i32
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

/// Saca los logs de whisper.cpp y ggml de stderr (por defecto imprimen mucho) y los
/// manda al crate `log`: sin un logger instalado, se descartan.
pub fn silence_native_logs() {
    whisper_rs::install_logging_hooks();
}

/// Backend de GPU con el que se compiló el binario, si hay alguno.
pub fn gpu_backend() -> Option<&'static str> {
    if cfg!(feature = "cuda") {
        Some("CUDA")
    } else if cfg!(feature = "vulkan") {
        Some("Vulkan")
    } else {
        None
    }
}

/// Una GPU que ve el backend compilado.
#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub name: String,
    pub vram_mb: u64,
}

/// GPUs que puede usar Whisper. Con Vulkan se consultan una vez (inicializar Vulkan
/// tarda); con CUDA whisper-rs no las lista y queda vacío.
pub fn gpu_devices() -> &'static [GpuDevice] {
    static DEVICES: OnceLock<Vec<GpuDevice>> = OnceLock::new();
    DEVICES.get_or_init(|| {
        #[cfg(feature = "vulkan")]
        {
            whisper_rs::vulkan::list_devices()
                .into_iter()
                .map(|d| GpuDevice {
                    name: d.name,
                    vram_mb: (d.vram.total / (1024 * 1024)) as u64,
                })
                .collect()
        }
        #[cfg(not(feature = "vulkan"))]
        {
            Vec::new()
        }
    })
}

/// Si Whisper va a usar la GPU. Con Vulkan además tiene que haber un dispositivo:
/// en una PC sin GPU compatible whisper.cpp sigue en CPU sin avisar.
pub fn gpu_available() -> bool {
    if cfg!(feature = "vulkan") {
        !gpu_devices().is_empty()
    } else {
        cfg!(feature = "cuda")
    }
}

/// Instrucciones de CPU con las que se compiló whisper.cpp ("AVX = 1 | AVX2 = 1 | ...").
pub fn system_info() -> &'static str {
    whisper_rs::print_system_info()
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
        // 0 es la ventana completa. Se fija en cada llamada: el estado se reutiliza.
        params.set_audio_ctx(if opts.fit_audio_ctx {
            audio_ctx_for(samples.len())
        } else {
            0
        });

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_frases_fantasma_solas_se_detectan() {
        assert!(is_hallucination("Gracias."));
        assert!(is_hallucination("¡Suscríbete!"));
        assert!(is_hallucination("  Thank you.  "));
        assert!(is_hallucination("Subtítulos por la comunidad de Amara.org"));
    }

    #[test]
    fn la_ventana_ajustada_cubre_el_audio_con_margen() {
        let segundos = |s: usize| s * WHISPER_SAMPLE_RATE as usize;
        // 1 s son 50 posiciones: con el margen y el mínimo, 128.
        assert_eq!(audio_ctx_for(segundos(1)), 128);
        // 11 s son 550: con el margen, 614, y redondeado, 640.
        assert_eq!(audio_ctx_for(segundos(11)), 640);
        // Desde unos 28 s ya es la ventana completa.
        assert_eq!(audio_ctx_for(segundos(29)), 1500);
        assert_eq!(audio_ctx_for(segundos(60)), 1500);
    }

    #[test]
    fn un_texto_real_que_las_contiene_no_se_descarta() {
        assert!(!is_hallucination("Gracias por la ayuda con el informe."));
        assert!(!is_hallucination("Hola, ¿qué tal? Gracias."));
        assert!(!is_hallucination(""));
    }
}
