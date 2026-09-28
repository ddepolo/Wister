//! Prueba de rendimiento: el usuario lee una frase y se transcribe con cada modelo
//! descargado, con GPU y con CPU, para ver cuál conviene en esa PC.

use std::time::Instant;

use serde::Serialize;
use wister_core::audio::Audio;
use wister_core::{models, stt};

use tauri::{AppHandle, Manager};

use crate::config::ConfigActual;
use crate::dictado::{cargar_engine, CanalDictado, Mensaje};

/// Si transcribir la frase tarda más que esto, el modelo se siente lento al dictar.
const ESPERA_ACEPTABLE_MS: u64 = 1000;

/// Frase para leer e idioma con que se transcribe. Sin números: Whisper puede escribir
/// "3" o "tres" y los dos están bien.
pub fn frase(idioma: &str) -> (&'static str, &'static str) {
    if idioma == "es" || idioma == "auto" {
        (
            "Mañana a la tarde tengo una reunión con el equipo de ventas para revisar el \
             presupuesto del próximo trimestre. Después le mando un correo a Martina con el \
             resumen y los pendientes.",
            "es",
        )
    } else {
        (
            "Tomorrow afternoon I have a meeting with the sales team to review the budget for \
             the next quarter. Afterwards I will send Martina an email with the summary and the \
             open items.",
            "en",
        )
    }
}

/// Empieza a grabar la frase y la devuelve, para mostrarla.
#[tauri::command]
pub fn iniciar_prueba_rendimiento(app: AppHandle) -> Result<String, String> {
    let idioma = app
        .try_state::<ConfigActual>()
        .and_then(|c| c.0.lock().ok().map(|c| c.idioma.clone()))
        .unwrap_or_default();
    let (frase, idioma) = frase(&idioma);
    enviar(&app, Mensaje::GrabarRendimiento { frase, idioma })?;
    Ok(frase.into())
}

/// Deja de grabar y mide; el avance llega por el evento `rendimiento`.
#[tauri::command]
pub fn medir_prueba_rendimiento(app: AppHandle) -> Result<(), String> {
    enviar(&app, Mensaje::MedirRendimiento)
}

#[tauri::command]
pub fn cancelar_prueba_rendimiento(app: AppHandle) -> Result<(), String> {
    enviar(&app, Mensaje::CancelarRendimiento)
}

fn enviar(app: &AppHandle, mensaje: Mensaje) -> Result<(), String> {
    app.try_state::<CanalDictado>()
        .and_then(|c| c.0.send(mensaje).ok())
        .ok_or_else(|| "Wister todavía está arrancando: probá de nuevo en unos segundos.".into())
}

/// Evento `rendimiento`, para la ventana de configuración.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Evento {
    Midiendo {
        modelo: String,
        gpu: bool,
        paso: usize,
        total: usize,
    },
    Medicion(Medicion),
    Fin {
        /// Voz que quedó después del VAD, que es lo que se transcribe.
        voz_ms: u64,
        recomendado: Option<String>,
    },
    Error {
        mensaje: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Medicion {
    pub modelo: String,
    pub gpu: bool,
    /// Carga y calentamiento: lo que se espera al elegir el modelo.
    pub carga_ms: u64,
    /// Lo que se espera en cada dictado después de soltar el atajo.
    pub transcripcion_ms: u64,
    /// Porcentaje de palabras de la frase que se transcribieron bien.
    pub aciertos: u32,
    pub texto: String,
    pub error: Option<String>,
}

/// Transcribe `voz` (a 16 kHz) con cada modelo descargado. No tiene que haber otro
/// modelo cargado: cada uno se carga, se mide y se libera antes del siguiente.
pub fn medir(voz: &Audio, frase: &str, idioma: &str, emitir: impl Fn(Evento)) -> Vec<Medicion> {
    let opciones = stt::Options {
        language: idioma.into(),
        ..Default::default()
    };
    let backends: &[bool] = if stt::gpu_available() {
        &[true, false]
    } else {
        &[false]
    };
    let pruebas: Vec<(&str, bool)> = models::CATALOG
        .iter()
        .filter(|m| m.is_downloaded())
        .flat_map(|m| backends.iter().map(move |&gpu| (m.name, gpu)))
        .collect();

    let mut mediciones = Vec::new();
    for (i, &(modelo, gpu)) in pruebas.iter().enumerate() {
        emitir(Evento::Midiendo {
            modelo: modelo.into(),
            gpu,
            paso: i + 1,
            total: pruebas.len(),
        });
        let m = medir_uno(modelo, gpu, voz, frase, &opciones);
        match &m.error {
            None => log::info!(
                "rendimiento: {modelo} con {}: carga {} ms, transcripción {} ms, {}% de aciertos: {:?}",
                if gpu { "GPU" } else { "CPU" },
                m.carga_ms,
                m.transcripcion_ms,
                m.aciertos,
                m.texto
            ),
            Some(e) => log::warn!("rendimiento: {modelo} (gpu: {gpu}) falló: {e}"),
        }
        emitir(Evento::Medicion(m.clone()));
        mediciones.push(m);
    }
    mediciones
}

fn medir_uno(
    modelo: &str,
    gpu: bool,
    voz: &Audio,
    frase: &str,
    opciones: &stt::Options,
) -> Medicion {
    let inicio = Instant::now();
    let resultado = cargar_engine(modelo, opciones, gpu).and_then(|mut engine| {
        let carga_ms = inicio.elapsed().as_millis() as u64;
        Ok((carga_ms, engine.transcribe(voz, opciones)?))
    });
    match resultado {
        Ok((carga_ms, t)) => Medicion {
            modelo: modelo.into(),
            gpu,
            carga_ms,
            transcripcion_ms: t.elapsed.as_millis() as u64,
            aciertos: aciertos(frase, &t.text),
            texto: t.text,
            error: None,
        },
        Err(e) => Medicion {
            modelo: modelo.into(),
            gpu,
            carga_ms: 0,
            transcripcion_ms: 0,
            aciertos: 0,
            texto: String::new(),
            error: Some(format!("{e:#}")),
        },
    }
}

/// El modelo de mejor calidad (el catálogo va de menor a mayor) que transcribe en un
/// tiempo aceptable con el backend que usa la app. Si ninguno llega, el más rápido.
pub fn recomendar(mediciones: &[Medicion], con_gpu: bool) -> Option<String> {
    let calidad = |nombre: &str| models::CATALOG.iter().position(|m| m.name == nombre);
    let validas: Vec<&Medicion> = mediciones
        .iter()
        .filter(|m| m.gpu == con_gpu && m.error.is_none())
        .collect();
    validas
        .iter()
        .filter(|m| m.transcripcion_ms <= ESPERA_ACEPTABLE_MS)
        .max_by_key(|m| calidad(&m.modelo))
        .or_else(|| validas.iter().min_by_key(|m| m.transcripcion_ms))
        .map(|m| m.modelo.clone())
}

/// Porcentaje de palabras de `referencia` bien transcriptas: 100 menos la tasa de error
/// por palabra (distancia de edición), sin contar mayúsculas ni puntuación.
pub fn aciertos(referencia: &str, texto: &str) -> u32 {
    let palabras = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let (r, t) = (palabras(referencia), palabras(texto));
    if r.is_empty() {
        return 0;
    }
    // Distancia de Levenshtein por palabras, con una sola fila.
    let mut fila: Vec<usize> = (0..=t.len()).collect();
    for (i, pr) in r.iter().enumerate() {
        let mut diagonal = fila[0];
        fila[0] = i + 1;
        for (j, pt) in t.iter().enumerate() {
            let arriba = fila[j + 1];
            fila[j + 1] = if pr == pt {
                diagonal
            } else {
                1 + diagonal.min(arriba).min(fila[j])
            };
            diagonal = arriba;
        }
    }
    let errores = fila[t.len()].min(r.len());
    (100 * (r.len() - errores) / r.len()) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn medicion(modelo: &str, gpu: bool, ms: u64) -> Medicion {
        Medicion {
            modelo: modelo.into(),
            gpu,
            carga_ms: 0,
            transcripcion_ms: ms,
            aciertos: 100,
            texto: String::new(),
            error: None,
        }
    }

    #[test]
    fn una_transcripcion_perfecta_tiene_todos_los_aciertos() {
        let frase = frase("es").0;
        assert_eq!(aciertos(frase, frase), 100);
        assert_eq!(aciertos("Hola, ¿qué tal?", "hola qué tal"), 100);
    }

    #[test]
    fn los_errores_bajan_los_aciertos() {
        assert_eq!(aciertos("uno dos tres cuatro", "uno dos tres"), 75);
        assert_eq!(aciertos("uno dos tres cuatro", "uno dos seis cuatro"), 75);
        assert_eq!(aciertos("uno dos", "cualquier otra cosa larga"), 0);
        assert_eq!(aciertos("uno dos", ""), 0);
        assert_eq!(aciertos("la reunión", "la reunion"), 50);
    }

    #[test]
    fn se_recomienda_el_mejor_modelo_que_llega_a_tiempo() {
        let m = [
            medicion("small", true, 200),
            medicion("large-v3-turbo-q5_0", true, 700),
            medicion("large-v3-turbo", true, 1800),
            medicion("large-v3-turbo", false, 900),
        ];
        assert_eq!(recomendar(&m, true).as_deref(), Some("large-v3-turbo-q5_0"));
    }

    #[test]
    fn si_ninguno_llega_a_tiempo_se_recomienda_el_mas_rapido() {
        let m = [
            medicion("small", false, 1500),
            medicion("large-v3-turbo-q5_0", false, 4000),
        ];
        assert_eq!(recomendar(&m, false).as_deref(), Some("small"));
        assert_eq!(recomendar(&m, true), None);
    }

    #[test]
    fn un_modelo_que_fallo_no_se_recomienda() {
        let mut roto = medicion("large-v3-turbo", true, 100);
        roto.error = Some("sin memoria".into());
        let m = [medicion("base", true, 100), roto];
        assert_eq!(recomendar(&m, true).as_deref(), Some("base"));
    }
}
