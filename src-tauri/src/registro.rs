//! Registro de funcionamiento (`wister.log` en la carpeta de logs de la app), para
//! diagnosticar fallas y rendimiento en otras PCs. Nunca se anota el texto dictado.
//!
//! Es un logger del crate `log`: la app escribe con `log::info!` y compañía, y
//! whisper.cpp y ggml llegan por el mismo camino (feature `log_backend` de whisper-rs).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use anyhow::{Context, Result};
use log::{Level, LevelFilter, Log, Metadata, Record};

pub const ARCHIVO: &str = "wister.log";
/// Al arrancar, si el registro pasa de `TAMANO_MAXIMO`, se renombra a este y se empieza otro.
pub const ANTERIOR: &str = "wister.anterior.log";
const TAMANO_MAXIMO: u64 = 2 * 1024 * 1024;

/// Los mensajes de whisper.cpp y ggml llegan con este prefijo de `target`.
const NATIVO: &str = "whisper_rs";

/// Mientras vale `true` también se anotan los mensajes informativos de whisper.cpp.
/// Solo se activa al cargar un modelo (dicen qué backend y cuánta memoria usa): en cada
/// dictado el VAD escribe varias líneas que no aportan.
static DETALLE_NATIVO: AtomicBool = AtomicBool::new(false);

static REGISTRO: Registro = Registro {
    archivo: Mutex::new(None),
};

struct Registro {
    archivo: Mutex<Option<File>>,
}

fn se_anota(nivel: Level, target: &str, detalle_nativo: bool) -> bool {
    if target.starts_with(NATIVO) {
        nivel <= Level::Warn || (detalle_nativo && nivel <= Level::Info)
    } else {
        nivel <= Level::Info
    }
}

impl Log for Registro {
    fn enabled(&self, metadata: &Metadata) -> bool {
        se_anota(
            metadata.level(),
            metadata.target(),
            DETALLE_NATIVO.load(Ordering::Relaxed),
        )
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mensaje = record.args().to_string();
        let nativo = record.target().starts_with(NATIVO);
        if nativo && record.level() == Level::Info && !nativo_util(&mensaje) {
            return;
        }
        let origen = if nativo { "[whisper] " } else { "" };
        let linea = format!("{} {:<5} {origen}{mensaje}\n", hora(), record.level());
        if cfg!(debug_assertions) {
            eprint!("{linea}");
        }
        if let Ok(mut archivo) = self.archivo.lock() {
            if let Some(a) = archivo.as_mut() {
                let _ = a.write_all(linea.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut archivo) = self.archivo.lock() {
            if let Some(a) = archivo.as_mut() {
                let _ = a.flush();
            }
        }
    }
}

/// Instala el logger y abre el archivo. Si no se puede abrir, la app sigue igual (en
/// desarrollo los mensajes salen por la consola).
pub fn iniciar(carpeta: &Path) {
    let _ = log::set_logger(&REGISTRO);
    log::set_max_level(LevelFilter::Info);
    match abrir(carpeta) {
        Ok(archivo) => {
            if let Ok(mut a) = REGISTRO.archivo.lock() {
                *a = Some(archivo);
            }
        }
        Err(e) => log::error!("{e:#}"),
    }
    let anterior = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("error interno: {info}");
        anterior(info);
    }));
}

fn abrir(carpeta: &Path) -> Result<File> {
    fs::create_dir_all(carpeta)
        .with_context(|| format!("no se pudo crear {}", carpeta.display()))?;
    let ruta = carpeta.join(ARCHIVO);
    if fs::metadata(&ruta).is_ok_and(|m| m.len() > TAMANO_MAXIMO) {
        let _ = fs::rename(&ruta, carpeta.join(ANTERIOR));
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ruta)
        .with_context(|| format!("no se pudo abrir el registro {}", ruta.display()))
}

/// De lo que whisper.cpp informa al cargar un modelo, lo que sirve para diagnosticar:
/// si pidió GPU, qué backend usa y cuánta memoria ocupa. El resto son sus hiperparámetros.
fn nativo_util(mensaje: &str) -> bool {
    [
        "use gpu",
        "flash attn",
        "backend",
        "total size",
        "model size",
    ]
    .iter()
    .any(|clave| mensaje.contains(clave))
}

/// Corre `f` anotando también los mensajes informativos de whisper.cpp.
pub fn con_detalle_nativo<T>(f: impl FnOnce() -> T) -> T {
    DETALLE_NATIVO.store(true, Ordering::Relaxed);
    let r = f();
    DETALLE_NATIVO.store(false, Ordering::Relaxed);
    r
}

/// Las últimas `cantidad` líneas del registro, contando el anterior si hace falta.
pub fn ultimas_lineas(carpeta: &Path, cantidad: usize) -> String {
    REGISTRO.flush();
    let leer = |nombre: &str| fs::read_to_string(carpeta.join(nombre)).unwrap_or_default();
    ultimas(&[leer(ANTERIOR), leer(ARCHIVO)].concat(), cantidad)
}

fn ultimas(texto: &str, cantidad: usize) -> String {
    let lineas: Vec<&str> = texto.lines().collect();
    lineas[lineas.len().saturating_sub(cantidad)..].join("\n")
}

pub fn carpeta(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    app.path().app_log_dir().ok()
}

/// Fecha y hora local con milisegundos: `2026-09-28 14:03:22.123`.
#[cfg(windows)]
pub fn hora() -> String {
    let t = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

/// Fuera de Windows alcanza con los segundos desde 1970.
#[cfg(not(windows))]
pub fn hora() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", t.as_secs(), t.subsec_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_solo_anota_advertencias_salvo_al_cargar_el_modelo() {
        let target = "whisper_rs::whisper_logging_hook";
        assert!(!se_anota(Level::Info, target, false));
        assert!(se_anota(Level::Warn, target, false));
        assert!(se_anota(Level::Info, target, true));
        assert!(!se_anota(Level::Debug, target, true));
    }

    #[test]
    fn de_la_carga_del_modelo_quedan_el_backend_y_la_memoria() {
        assert!(nativo_util(
            "whisper_backend_init_gpu: using Vulkan0 backend"
        ));
        assert!(nativo_util(
            "whisper_model_load: model size    =  573.40 MB"
        ));
        assert!(!nativo_util("whisper_model_load: n_vocab       = 51866"));
    }

    #[test]
    fn la_app_anota_desde_info() {
        assert!(se_anota(Level::Info, "wister_app::dictado", false));
        assert!(se_anota(Level::Error, "wister_app::dictado", false));
        assert!(!se_anota(Level::Debug, "wister_app::dictado", false));
    }

    #[test]
    fn las_ultimas_lineas_respetan_el_orden() {
        assert_eq!(ultimas("a\nb\nc\nd\n", 2), "c\nd");
        assert_eq!(ultimas("a\nb", 5), "a\nb");
        assert_eq!(ultimas("", 3), "");
    }
}
