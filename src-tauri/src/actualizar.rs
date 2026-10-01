//! Actualizaciones desde GitHub Releases, solo cuando el usuario las pide desde
//! Configuración: la app no se conecta sola.
//!
//! El `latest.json` del último release trae dos entradas: `windows-x86_64` (la de Vulkan)
//! y `windows-x86_64-cpu`, que buscan las instalaciones de solo CPU de hasta la 0.3.0.
//! Desde que la de Vulkan también abre sin drivers (ver `gpu.rs`) hay un solo instalador
//! y las dos apuntan a él.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Updater, UpdaterExt};

#[derive(Serialize)]
pub struct Disponible {
    version: String,
    notas: Option<String>,
}

/// Evento `actualizacion` mientras se baja el instalador.
#[derive(Clone, Serialize)]
struct Progreso {
    bajado: u64,
    total: Option<u64>,
}

/// Clave del `latest.json` para esta variante, o `None` para la de por defecto.
/// La build con CUDA no se distribuye: usa la de Vulkan.
fn variante() -> Option<String> {
    if cfg!(any(feature = "vulkan", feature = "cuda")) {
        None
    } else {
        tauri_plugin_updater::target().map(|t| format!("{t}-cpu"))
    }
}

fn actualizador(app: &AppHandle) -> Result<Updater, String> {
    let mut builder = app.updater_builder().timeout(Duration::from_secs(30));
    if let Some(v) = variante() {
        builder = builder.target(v);
    }
    builder
        .build()
        .map_err(|e| format!("No se pudo preparar el actualizador: {e}"))
}

fn sin_conexion(e: tauri_plugin_updater::Error) -> String {
    format!("No se pudo buscar actualizaciones ({e}). Fijate que tengas conexión a internet.")
}

#[tauri::command]
pub async fn buscar_actualizacion(app: AppHandle) -> Result<Option<Disponible>, String> {
    let update = actualizador(&app)?.check().await.map_err(sin_conexion)?;
    Ok(update.map(|u| Disponible {
        version: u.version,
        notas: u.body,
    }))
}

/// Baja la versión nueva, verifica la firma y ejecuta el instalador. En Windows el
/// instalador cierra Wister y lo vuelve a abrir, así que este comando no vuelve.
#[tauri::command]
pub async fn instalar_actualizacion(app: AppHandle) -> Result<(), String> {
    let Some(update) = actualizador(&app)?.check().await.map_err(sin_conexion)? else {
        return Err("Ya tenés la última versión.".into());
    };
    let mut bajado = 0u64;
    let mut ultimo = Instant::now() - Duration::from_secs(1);
    update
        .download_and_install(
            |tramo, total| {
                bajado += tramo as u64;
                // Un evento cada 150 ms alcanza para la barra de progreso.
                if ultimo.elapsed() >= Duration::from_millis(150) {
                    ultimo = Instant::now();
                    let _ = app.emit("actualizacion", Progreso { bajado, total });
                }
            },
            || {},
        )
        .await
        .map_err(|e| format!("No se pudo instalar la actualización: {e}"))?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_variante_busca_su_entrada() {
        let v = variante();
        if cfg!(any(feature = "vulkan", feature = "cuda")) {
            assert_eq!(v, None);
        } else if let Some(v) = v {
            assert!(v.ends_with("-cpu"), "{v}");
        }
    }
}
