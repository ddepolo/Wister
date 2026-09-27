//! Captura de micrófono y preparación del audio para Whisper (16 kHz, mono, f32).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};

/// Frecuencia de muestreo que espera Whisper.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Umbral de [`Audio::has_voice`]. En las pruebas de la Fase 0 el ambiente dio un RMS
/// de 0,001 y la voz normal, picos de 0,05 a 0,2.
pub const VOICE_RMS_THRESHOLD: f32 = 0.01;
/// Mínimo de audio por encima del umbral para considerar que hubo voz.
pub const VOICE_MIN_MS: u32 = 200;

/// Audio mono en f32 a una frecuencia de muestreo dada.
#[derive(Debug, Clone)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Audio {
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    /// Devuelve el audio remuestreado a 16 kHz. Si ya está a 16 kHz lo copia tal cual.
    pub fn to_whisper(&self) -> Result<Audio> {
        Ok(Audio {
            samples: resample(&self.samples, self.sample_rate, WHISPER_SAMPLE_RATE)?,
            sample_rate: WHISPER_SAMPLE_RATE,
        })
    }

    /// Nivel RMS de todo el audio (0.0 = silencio).
    pub fn rms(&self) -> f32 {
        rms(&self.samples)
    }

    /// Si hay voz: al menos [`VOICE_MIN_MS`] en ventanas de 20 ms con RMS por encima de
    /// [`VOICE_RMS_THRESHOLD`]. Un RMS global no sirve porque las pausas diluyen la voz.
    ///
    /// Es un filtro grueso para no mandarle a Whisper audio sin voz, que siempre inventa
    /// algo ("Gracias.", "¡Suscríbete!"). No reemplaza a un VAD.
    pub fn has_voice(&self) -> bool {
        let window = (self.sample_rate as usize / 50).max(1);
        let needed = (VOICE_MIN_MS as usize).div_ceil(20);
        self.samples
            .chunks(window)
            .filter(|w| rms(w) > VOICE_RMS_THRESHOLD)
            .count()
            >= needed
    }

    pub fn save_wav(&self, path: &Path) -> Result<()> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .with_context(|| format!("no se pudo crear {}", path.display()))?;
        for &s in &self.samples {
            writer.write_sample(i16::from_sample(s.clamp(-1.0, 1.0)))?;
        }
        writer.finalize()?;
        Ok(())
    }

    /// Lee un WAV (PCM entero o float, cualquier cantidad de canales) y lo pasa a mono.
    pub fn load_wav(path: &Path) -> Result<Audio> {
        let mut reader = hound::WavReader::open(path)
            .with_context(|| format!("no se pudo abrir {}", path.display()))?;
        let spec = reader.spec();
        let interleaved: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
            hound::SampleFormat::Int => {
                let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / scale))
                    .collect::<Result<_, _>>()?
            }
        };
        Ok(Audio {
            samples: downmix(&interleaved, spec.channels as usize),
            sample_rate: spec.sample_rate,
        })
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Promedia los canales de un buffer intercalado.
pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Remuestreo sincrónico por FFT (la relación entre frecuencias es fija).
pub fn resample(samples: &[f32], from: u32, to: u32) -> Result<Vec<f32>> {
    if from == to || samples.is_empty() {
        return Ok(samples.to_vec());
    }
    let mut resampler = Fft::<f32>::new(from as usize, to as usize, 1024, 1, FixedSync::Input)
        .map_err(|e| anyhow!("no se pudo crear el remuestreador: {e}"))?;
    let input = vec![samples.to_vec()];
    let adapter = SequentialSliceOfVecs::new(&input, 1, samples.len())
        .map_err(|e| anyhow!("buffer de entrada inválido: {e}"))?;
    let output = resampler
        .process_all(&adapter, samples.len(), None)
        .map_err(|e| anyhow!("falló el remuestreo: {e}"))?;
    Ok(output.take_data())
}

/// Un micrófono disponible.
#[derive(Debug, Clone)]
pub struct InputDevice {
    pub name: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Result<Vec<InputDevice>> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().map(|d| d.to_string());
    let devices = host
        .input_devices()
        .context("no se pudieron listar los dispositivos de entrada")?;
    Ok(devices
        .map(|d| {
            let name = d.to_string();
            InputDevice {
                is_default: default_name.as_deref() == Some(name.as_str()),
                name,
            }
        })
        .collect())
}

fn find_device(query: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    let Some(query) = query else {
        return host
            .default_input_device()
            .ok_or_else(|| anyhow!("no hay un micrófono por defecto configurado"));
    };
    let needle = query.to_lowercase();
    host.input_devices()?
        .find(|d| d.to_string().to_lowercase().contains(&needle))
        .ok_or_else(|| anyhow!("no se encontró ningún micrófono que contenga \"{query}\""))
}

/// Capacidad del ring buffer entre el hilo de audio y el que junta las muestras:
/// 2 s a 48 kHz, de sobra para los 10 ms entre vaciados.
const RING_CAPACITY: usize = 96_000;
const DRAIN_INTERVAL: Duration = Duration::from_millis(10);

/// Grabación en curso. Se detiene con [`Recording::stop`].
pub struct Recording {
    stream: cpal::Stream,
    stop: Arc<AtomicBool>,
    collector: JoinHandle<Vec<f32>>,
    dropped: Arc<AtomicUsize>,
    sample_rate: u32,
    pub device_name: String,
}

/// Empieza a grabar del micrófono indicado (o del de por defecto).
///
/// `device` se compara como subcadena, sin distinguir mayúsculas, contra el nombre
/// del dispositivo.
pub fn start_recording(device: Option<&str>) -> Result<Recording> {
    start_recording_with_levels(device, |_| {})
}

/// Cada cuánto se informa el nivel en [`start_recording_with_levels`].
pub const LEVEL_INTERVAL: Duration = Duration::from_millis(50);

/// Como [`start_recording`], pero llama a `on_level` con el RMS de cada tramo de
/// [`LEVEL_INTERVAL`], para dibujar la forma de onda mientras se graba. Se llama
/// desde el hilo que junta las muestras, no desde el de audio.
pub fn start_recording_with_levels(
    device: Option<&str>,
    mut on_level: impl FnMut(f32) + Send + 'static,
) -> Result<Recording> {
    let device = find_device(device)?;
    let device_name = device.to_string();
    let config = device
        .default_input_config()
        .context("el micrófono no informó una configuración de entrada")?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;

    // El callback corre en el hilo de audio del sistema: no puede reservar memoria
    // ni bloquearse, así que solo mezcla a mono y escribe en un ring buffer sin locks.
    let (producer, mut consumer) = HeapRb::<f32>::new(RING_CAPACITY).split();
    let dropped = Arc::new(AtomicUsize::new(0));
    let err_fn = |err| eprintln!("error en el stream de audio: {err}");

    macro_rules! build {
        ($t:ty) => {{
            let mut producer = producer;
            let dropped = dropped.clone();
            device.build_input_stream(
                config.clone().into(),
                move |data: &[$t], _: &_| {
                    for frame in data.chunks_exact(channels) {
                        let mono = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>()
                            / channels as f32;
                        if producer.try_push(mono).is_err() {
                            dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                err_fn,
                None,
            )
        }};
    }

    let stream = match config.sample_format() {
        SampleFormat::F32 => build!(f32),
        SampleFormat::I16 => build!(i16),
        SampleFormat::U16 => build!(u16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I8 => build!(i8),
        SampleFormat::U8 => build!(u8),
        other => bail!("formato de muestra no soportado: {other}"),
    }
    .context("no se pudo abrir el stream del micrófono")?;

    let stop = Arc::new(AtomicBool::new(false));
    let collector = {
        let stop = stop.clone();
        std::thread::Builder::new()
            .name("wister-audio".into())
            .spawn(move || {
                let mut samples: Vec<f32> = Vec::new();
                let per_level =
                    ((sample_rate as u128 * LEVEL_INTERVAL.as_millis() / 1000) as usize).max(1);
                let mut level_from = 0;
                loop {
                    // Se lee el flag antes de vaciar, para no perder lo que llegó justo antes de parar.
                    let stopping = stop.load(Ordering::Acquire);
                    samples.extend(consumer.pop_iter());
                    if stopping {
                        return samples;
                    }
                    while samples.len() - level_from >= per_level {
                        on_level(rms(&samples[level_from..level_from + per_level]));
                        level_from += per_level;
                    }
                    std::thread::sleep(DRAIN_INTERVAL);
                }
            })
            .context("no se pudo crear el hilo de captura")?
    };

    stream.play().context("no se pudo iniciar la grabación")?;

    Ok(Recording {
        stream,
        stop,
        collector,
        dropped,
        sample_rate,
        device_name,
    })
}

impl Recording {
    /// Detiene la grabación y devuelve todo el audio capturado, en mono y a la
    /// frecuencia nativa del dispositivo.
    pub fn stop(self) -> Audio {
        drop(self.stream);
        self.stop.store(true, Ordering::Release);
        let samples = self.collector.join().unwrap_or_default();
        let dropped = self.dropped.load(Ordering::Relaxed);
        if dropped > 0 {
            eprintln!("se perdieron {dropped} muestras de audio (ring buffer lleno)");
        }
        Audio {
            samples,
            sample_rate: self.sample_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_promedia_canales() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(downmix(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn resample_48k_a_16k_mantiene_duracion() {
        let from = 48_000;
        let secs = 2.0;
        let n = (from as f32 * secs) as usize;
        let sine: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / from as f32).sin() * 0.5)
            .collect();
        let out = resample(&sine, from, WHISPER_SAMPLE_RATE).unwrap();
        assert_eq!(out.len(), (WHISPER_SAMPLE_RATE as f32 * secs) as usize);
        // Una senoidal de amplitud 0.5 tiene RMS ~0.354; el remuestreo no debe cambiarlo.
        assert!((rms(&out[1000..out.len() - 1000]) - 0.354).abs() < 0.01);
    }

    fn senoidal(amplitud: f32, secs: f32, rate: u32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize)
            .map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin() * amplitud)
            .collect()
    }

    #[test]
    fn silencio_y_ruido_de_ambiente_no_son_voz() {
        let rate = 16_000;
        let silencio = Audio {
            samples: vec![0.0; rate as usize * 3],
            sample_rate: rate,
        };
        assert!(!silencio.has_voice());
        let ambiente = Audio {
            samples: senoidal(0.0015, 3.0, rate),
            sample_rate: rate,
        };
        assert!(!ambiente.has_voice());
    }

    #[test]
    fn medio_segundo_de_voz_entre_silencio_es_voz() {
        let rate = 48_000;
        let mut samples = vec![0.0; rate as usize * 2];
        samples.extend(senoidal(0.1, 0.5, rate));
        samples.extend(vec![0.0; rate as usize * 2]);
        let audio = Audio {
            samples,
            sample_rate: rate,
        };
        assert!(audio.rms() < VOICE_RMS_THRESHOLD * 3.0);
        assert!(audio.has_voice());
    }

    #[test]
    fn un_golpe_corto_no_es_voz() {
        let rate = 16_000;
        let mut samples = vec![0.0; rate as usize];
        samples.extend(senoidal(0.3, 0.1, rate));
        samples.extend(vec![0.0; rate as usize]);
        let audio = Audio {
            samples,
            sample_rate: rate,
        };
        assert!(!audio.has_voice());
    }

    #[test]
    fn wav_ida_y_vuelta() {
        let dir = std::env::temp_dir().join(format!("wister-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prueba.wav");
        let audio = Audio {
            samples: vec![0.0, 0.25, -0.25, 0.5],
            sample_rate: 16_000,
        };
        audio.save_wav(&path).unwrap();
        let back = Audio::load_wav(&path).unwrap();
        assert_eq!(back.sample_rate, 16_000);
        for (a, b) in audio.samples.iter().zip(&back.samples) {
            assert!((a - b).abs() < 1e-3);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
