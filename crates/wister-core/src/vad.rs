//! Detector de voz: Silero VAD, que corre con el cÃƒÂ³digo de whisper.cpp.
//!
//! Se usa antes de transcribir para quedarse solo con los tramos que tienen voz:
//! recorta los silencios (menos audio para Whisper y menos texto inventado al
//! final) y, si no hay voz, no se transcribe nada.
//!
//! whisper.cpp tambiÃƒÂ©n puede aplicar el VAD dentro de `whisper_full`, pero whisper-rs
//! usa `whisper_full_with_state`, que lo ignora. Por eso se usa la API independiente.
//!
//! El modelo pesa menos de 1 MB, asÃƒÂ­ que viaja dentro del binario en vez de
//! descargarse: funciona desde el primer arranque y sin red. whisper.cpp solo lo puede
//! leer de un archivo, por eso se escribe en la carpeta de modelos la primera vez.
//!
//! Origen: <https://huggingface.co/ggml-org/whisper-vad> (`ggml-silero-v6.2.0.bin`,
//! SHA-256 `2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`).
//! Silero VAD tiene licencia MIT: <https://github.com/snakers4/silero-vad>.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

use crate::audio::Audio;
use crate::models;

const MODEL: &[u8] = include_bytes!("../assets/ggml-silero-v6.2.0.bin");
const FILE_NAME: &str = "ggml-silero-v6.2.0.bin";

/// Silencio que se deja entre dos tramos con voz, para que Whisper no junte palabras.
const GAP_MS: u32 = 100;

/// Ruta del modelo de VAD, escribiÃƒÂ©ndolo en la carpeta de modelos si hace falta.
pub fn model_path() -> Result<PathBuf> {
    let dir = models::models_dir()?;
    let path = dir.join(FILE_NAME);
    let al_dia = std::fs::metadata(&path)
        .map(|m| m.len() == MODEL.len() as u64)
        .unwrap_or(false);
    if !al_dia {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("no se pudo crear {}", dir.display()))?;
        std::fs::write(&path, MODEL)
            .with_context(|| format!("no se pudo escribir {}", path.display()))?;
    }
    Ok(path)
}

pub struct Vad {
    ctx: WhisperVadContext,
}

impl Vad {
    pub fn load() -> Result<Self> {
        let path = model_path()?;
        let path = path
            .to_str()
            .context("la ruta del modelo de VAD no es UTF-8 vÃƒÂ¡lido")?;
        // Es un modelo chico y en CPU no le quita GPU a Whisper. Con más hilos es más
        // lento, porque coordinarlos cuesta más de lo que ahorran: para 10 s de audio,
        // 1 hilo tarda ~20 ms, 2 hilos ~80 ms y 4 hilos ~180 ms.
        let mut params = WhisperVadContextParams::new();
        params.set_n_threads(1);
        let ctx = WhisperVadContext::new(path, params)
            .map_err(|e| anyhow!("no se pudo cargar el modelo de VAD: {e:?}"))?;
        Ok(Self { ctx })
    }

    /// Devuelve solo los tramos con voz de `audio` (a 16 kHz), separados por un
    /// silencio corto. Si no hay voz, devuelve un audio vacÃƒÂ­o.
    pub fn speech(&mut self, audio: &Audio) -> Result<Audio> {
        let mut params = WhisperVadParams::new();
        // El default (30 ms) a veces se come la primera o la ÃƒÂºltima sÃƒÂ­laba.
        params.set_speech_pad(100);
        let segments = self
            .ctx
            .segments_from_samples(params, &audio.samples)
            .map_err(|e| anyhow!("fallÃƒÂ³ la detecciÃƒÂ³n de voz: {e:?}"))?;
        // whisper.cpp da los tiempos en centÃƒÂ©simas de segundo.
        let tramos: Vec<(f32, f32)> = segments.map(|s| (s.start * 10.0, s.end * 10.0)).collect();
        Ok(Audio {
            samples: join_segments(&audio.samples, &tramos, audio.sample_rate),
            sample_rate: audio.sample_rate,
        })
    }
}

/// Une los tramos (inicio y fin en ms) de `samples`, con [`GAP_MS`] de silencio entre cada uno.
fn join_segments(samples: &[f32], tramos_ms: &[(f32, f32)], rate: u32) -> Vec<f32> {
    let a_muestra = |ms: f32| ((ms.max(0.0) * rate as f32 / 1000.0) as usize).min(samples.len());
    let gap = vec![0.0; (GAP_MS * rate / 1000) as usize];
    let mut voz = Vec::new();
    for &(inicio, fin) in tramos_ms {
        let (a, b) = (a_muestra(inicio), a_muestra(fin));
        if b <= a {
            continue;
        }
        if !voz.is_empty() {
            voz.extend_from_slice(&gap);
        }
        voz.extend_from_slice(&samples[a..b]);
    }
    voz
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_modelo_embebido_es_el_esperado() {
        // TamaÃƒÂ±o publicado en Hugging Face para ggml-silero-v6.2.0.bin.
        assert_eq!(MODEL.len(), 885_098);
        assert_eq!(&MODEL[..4], b"lmgg");
    }

    #[test]
    fn sin_tramos_no_queda_audio() {
        assert!(join_segments(&[0.5; 16_000], &[], 16_000).is_empty());
    }

    #[test]
    fn los_tramos_se_unen_con_un_silencio_corto() {
        let samples: Vec<f32> = (0..16_000).map(|i| i as f32).collect();
        // 100Ã¢â‚¬â€œ200 ms y 500Ã¢â‚¬â€œ600 ms: 1600 muestras cada uno, mÃƒÂ¡s 1600 de silencio en el medio.
        let voz = join_segments(&samples, &[(100.0, 200.0), (500.0, 600.0)], 16_000);
        assert_eq!(voz.len(), 1600 * 3);
        assert_eq!(voz[0], 1600.0);
        assert_eq!(voz[1600], 0.0);
        assert_eq!(voz[3200], 8000.0);
    }

    #[test]
    fn los_tramos_fuera_del_audio_se_recortan() {
        let voz = join_segments(&[1.0; 1600], &[(50.0, 900.0)], 16_000);
        assert_eq!(voz.len(), 1600 - 800);
    }
}
