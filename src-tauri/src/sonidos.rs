//! Sonidos cortos al empezar y al terminar de grabar.
//!
//! Se generan en memoria (dos tonos con fundido) en vez de venir de archivos: son
//! pocos bytes y así no hay recursos que empaquetar.

// Fuera de Windows todavía no suenan: el WAV solo lo usan los tests.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::OnceLock;

const FRECUENCIA: u32 = 44_100;

#[derive(Clone, Copy)]
pub enum Sonido {
    /// Dos tonos que suben.
    Inicio,
    /// Dos tonos que bajan.
    Fin,
}

impl Sonido {
    fn wav(self) -> &'static [u8] {
        static INICIO: OnceLock<Vec<u8>> = OnceLock::new();
        static FIN: OnceLock<Vec<u8>> = OnceLock::new();
        match self {
            Sonido::Inicio => INICIO.get_or_init(|| wav(&[(660.0, 70), (880.0, 90)])),
            Sonido::Fin => FIN.get_or_init(|| wav(&[(880.0, 70), (660.0, 90)])),
        }
    }
}

/// WAV PCM de 16 bits mono con una secuencia de tonos (frecuencia en Hz, duración en ms).
fn wav(tonos: &[(f32, u32)]) -> Vec<u8> {
    let mut muestras: Vec<i16> = Vec::new();
    for &(hz, ms) in tonos {
        let n = (FRECUENCIA * ms / 1000) as usize;
        let fundido = (FRECUENCIA as usize / 200).min(n / 2); // 5 ms, para que no haga clic
        for i in 0..n {
            let envolvente = (i.min(n - 1 - i) as f32 / fundido as f32).min(1.0);
            let t = i as f32 / FRECUENCIA as f32;
            let valor = (t * hz * std::f32::consts::TAU).sin() * 0.18 * envolvente;
            muestras.push((valor * i16::MAX as f32) as i16);
        }
    }
    let datos = (muestras.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + datos as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + datos).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // tamaño del bloque fmt
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&FRECUENCIA.to_le_bytes());
    b.extend_from_slice(&(FRECUENCIA * 2).to_le_bytes()); // bytes por segundo
    b.extend_from_slice(&2u16.to_le_bytes()); // bytes por muestra
    b.extend_from_slice(&16u16.to_le_bytes()); // bits por muestra
    b.extend_from_slice(b"data");
    b.extend_from_slice(&datos.to_le_bytes());
    for m in muestras {
        b.extend_from_slice(&m.to_le_bytes());
    }
    b
}

/// Reproduce el sonido sin esperar a que termine.
#[cfg(windows)]
pub fn reproducir(sonido: Sonido) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // Con SND_MEMORY + SND_ASYNC el buffer tiene que seguir vivo mientras suena: es estático.
    let wav = sonido.wav();
    unsafe {
        PlaySoundW(
            wav.as_ptr() as *const u16,
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

#[cfg(not(windows))]
pub fn reproducir(_sonido: Sonido) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_wav_tiene_cabecera_y_largo_correctos() {
        let w = wav(&[(440.0, 100)]);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        let muestras = (FRECUENCIA / 10) as usize;
        assert_eq!(w.len(), 44 + muestras * 2);
        assert_eq!(
            u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize,
            muestras * 2
        );
    }

    #[test]
    fn los_sonidos_empiezan_y_terminan_en_silencio() {
        let w = Sonido::Inicio.wav();
        let primera = i16::from_le_bytes([w[44], w[45]]);
        let ultima = i16::from_le_bytes([w[w.len() - 2], w[w.len() - 1]]);
        assert_eq!(primera, 0);
        assert!(ultima.abs() < 100);
    }
}
