//! Núcleo de Wister: captura de audio, modelos y transcripción local con Whisper.
//!
//! Lo usan la CLI (`wister-cli`) y la app (`src-tauri`).

pub mod audio;
pub mod models;
pub mod stt;
pub mod vad;
