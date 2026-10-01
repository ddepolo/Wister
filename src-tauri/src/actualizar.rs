//! Actualizaciones desde GitHub Releases. Se buscan cuando el usuario toca el botón de
//! Configuración y, si "Buscar actualizaciones automáticamente" está prendida, al minuto
//! de abrir Wister y después una vez por día. Buscar es pedir `latest.json`: no se manda
//! ningún dato del usuario. Instalar siempre lo decide el usuario.
//!
//! El `latest.json` del último release trae dos entradas: `windows-x86_64` (la de Vulkan)
//! y `windows-x86_64-cpu`, que buscan las instalaciones de solo CPU de hasta la 0.3.0.
//! Desde que la de Vulkan también abre sin drivers (ver `gpu.rs`) hay un solo instalador
//! y las dos apuntan a él.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Updater, UpdaterExt};

use crate::config::ConfigActual;

/// Después de abrir Wister: que no compita con la carga del modelo.
const PRIMERA_BUSQUEDA: Duration = Duration::from_secs(60);
const ENTRE_BUSQUEDAS: Duration = Duration::from_secs(24 * 60 * 60);
/// Cada cuánto se fija si ya toca buscar. Contar con la hora del sistema y no con un
/// `sleep` de un día hace que funcione aunque la PC se suspenda en el medio.
const REVISAR_CADA: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Serialize)]
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

/// La versión nueva que encontró la búsqueda automática, para el cartel de Inicio.
#[derive(Default)]
pub struct Pendiente(Mutex<Option<Disponible>>);

/// Para la ventana que se abre después de la búsqueda (el aviso ya pasó).
#[tauri::command]
pub fn actualizacion_pendiente(app: AppHandle) -> Option<Disponible> {
    app.try_state::<Pendiente>()
        .and_then(|p| p.0.lock().ok().and_then(|d| d.clone()))
}

/// Busca actualizaciones en segundo plano mientras la opción esté prendida. Si hay una
/// versión nueva la guarda y avisa con el evento `actualizacion_disponible`.
pub fn buscar_periodicamente(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("wister-actualizaciones".into())
        .spawn(move || {
            std::thread::sleep(PRIMERA_BUSQUEDA);
            let mut ultima: Option<SystemTime> = None;
            loop {
                let toca =
                    ultima.is_none_or(|u| u.elapsed().unwrap_or_default() >= ENTRE_BUSQUEDAS);
                if toca && activada(&app) {
                    ultima = Some(SystemTime::now());
                    tauri::async_runtime::block_on(buscar_en_segundo_plano(&app));
                }
                std::thread::sleep(REVISAR_CADA);
            }
        });
}

fn activada(app: &AppHandle) -> bool {
    app.try_state::<ConfigActual>()
        .and_then(|c| c.0.lock().ok().map(|c| c.buscar_actualizaciones))
        .unwrap_or(false)
}

async fn buscar_en_segundo_plano(app: &AppHandle) {
    let resultado = match actualizador(app) {
        Ok(a) => a.check().await.map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };
    match resultado {
        Ok(Some(update)) => {
            log::info!("actualizaciones: hay una versión nueva, {}", update.version);
            let disponible = Disponible {
                version: update.version,
                notas: update.body,
            };
            if let Some(p) = app.try_state::<Pendiente>() {
                if let Ok(mut d) = p.0.lock() {
                    *d = Some(disponible.clone());
                }
            }
            let _ = app.emit("actualizacion_disponible", disponible);
        }
        Ok(None) => log::info!("actualizaciones: Wister está al día"),
        // Sin internet es normal: no se le avisa al usuario.
        Err(e) => log::info!("actualizaciones: no se pudo buscar ({e})"),
    }
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
