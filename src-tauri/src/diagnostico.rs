//! Exportar un informe de diagnóstico: un archivo de texto con los datos de la PC, la
//! configuración y el registro de funcionamiento, para pedir ayuda desde otra máquina.

use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};
use wister_core::{audio, models, stt};

use crate::config;
use crate::dictado::EstadoActual;
use crate::registro;
use crate::sistema::{self, Sistema};

/// Líneas del registro que entran en el informe (unos días de uso normal).
const LINEAS_DE_REGISTRO: usize = 3000;

/// Guarda el informe en Descargas, lo muestra en el Explorador y devuelve la ruta.
// `async`: listar micrófonos y leer el registro no tienen que trabar la ventana.
#[tauri::command(async)]
pub fn exportar_diagnostico(app: AppHandle) -> Result<String, String> {
    let ruta = exportar(&app).map_err(|e| format!("{e:#}"))?;
    log::info!("diagnóstico exportado en {}", ruta.display());
    mostrar_en_explorador(&ruta);
    Ok(ruta.display().to_string())
}

fn exportar(app: &AppHandle) -> Result<PathBuf> {
    let texto = informe(app);
    let carpeta = app
        .path()
        .download_dir()
        .or_else(|_| app.path().desktop_dir())
        .context("no se encontró la carpeta de Descargas")?;
    let hora = registro::hora();
    let sello: String = hora
        .chars()
        .take(19)
        .filter_map(|c| match c {
            '0'..='9' => Some(c),
            ' ' => Some('-'),
            _ => None,
        })
        .collect();
    let ruta = carpeta.join(format!("wister-diagnostico-{sello}.txt"));
    std::fs::write(&ruta, texto)
        .with_context(|| format!("no se pudo guardar {}", ruta.display()))?;
    Ok(ruta)
}

fn informe(app: &AppHandle) -> String {
    let sistema = Sistema::detectar();
    let mut s = String::new();
    let _ = writeln!(s, "Diagnóstico de Wister");
    let _ = writeln!(s, "Generado: {}", registro::hora());
    let _ = writeln!(
        s,
        "Versión: {}{}",
        app.package_info().version,
        if cfg!(debug_assertions) {
            " (desarrollo)"
        } else {
            ""
        }
    );
    let _ = writeln!(s, "\n## Sistema");
    let _ = writeln!(s, "Windows: {}", sistema.windows);
    let _ = writeln!(s, "CPU: {} ({} hilos)", sistema.cpu, sistema.nucleos);
    let _ = writeln!(s, "RAM: {} MB", sistema.ram_mb);
    for placa in &sistema.placas {
        let _ = writeln!(s, "Placa de video: {placa}");
    }
    let _ = writeln!(s, "GPU para Whisper: {}", sistema::descripcion_gpu());
    let _ = writeln!(s, "whisper.cpp: {}", stt::system_info().trim());

    let _ = writeln!(s, "\n## Estado");
    if let Some(estado) = app.try_state::<EstadoActual>() {
        if let Ok(e) = estado.0.lock() {
            let _ = writeln!(s, "{e:?}");
        }
    }
    let _ = writeln!(s, "\n## Configuración");
    let config = config::obtener_config(app.clone());
    let _ = writeln!(
        s,
        "{}",
        serde_json::to_string_pretty(&config).unwrap_or_default()
    );
    let _ = writeln!(s, "\n## Modelos");
    for m in models::CATALOG {
        let _ = writeln!(
            s,
            "{}: {}",
            m.name,
            if m.is_downloaded() {
                "descargado"
            } else {
                "no descargado"
            }
        );
    }
    let _ = writeln!(s, "\n## Micrófonos");
    match audio::list_input_devices() {
        Ok(microfonos) => {
            for m in microfonos {
                let marca = if m.is_default {
                    " (predeterminado)"
                } else {
                    ""
                };
                let _ = writeln!(s, "{}{marca}", m.name);
            }
        }
        Err(e) => {
            let _ = writeln!(s, "no se pudieron listar: {e:#}");
        }
    }
    let _ = writeln!(s, "\n## Registro (sin el texto dictado)");
    match registro::carpeta(app) {
        Some(carpeta) => s.push_str(&registro::ultimas_lineas(&carpeta, LINEAS_DE_REGISTRO)),
        None => s.push_str("no se encontró la carpeta del registro"),
    }
    s.push('\n');
    s
}

#[cfg(windows)]
fn mostrar_en_explorador(ruta: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    // `raw_arg`: el Explorador no entiende las comillas que pondría `arg` alrededor de todo.
    let _ = std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", ruta.display()))
        .spawn();
}

#[cfg(not(windows))]
fn mostrar_en_explorador(_ruta: &std::path::Path) {}
