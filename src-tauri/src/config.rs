//! Configuración persistente (`config.json` en la carpeta de configuración de la app)
//! y los comandos que usa la ventana de configuración.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use wister_core::{audio, models};

use crate::dictado::{self, CanalDictado, Mensaje};
use crate::hotkey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `None`: el recomendado según haya GPU o no.
    pub modelo: Option<String>,
    /// `None`: el micrófono predeterminado de Windows.
    pub microfono: Option<String>,
    /// Código de idioma de Whisper ("es", "en"...) o "auto".
    pub idioma: String,
    /// Si ya se completó el asistente de primer uso. Falta en las configuraciones
    /// anteriores al asistente, y entonces vale `false`: se muestra una vez.
    pub asistente_completo: bool,
    pub mostrar_overlay: bool,
    /// Sonido corto al confirmarse la grabación y al soltar el atajo.
    pub sonidos: bool,
    /// Teclas del atajo (códigos virtuales de Windows, con lado).
    pub atajo: Vec<u32>,
    /// Si cada dictado se guarda en el historial (`historial.db`).
    pub guardar_historial: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            modelo: None,
            microfono: None,
            idioma: "es".into(),
            asistente_completo: false,
            mostrar_overlay: true,
            sonidos: false,
            atajo: hotkey::ATAJO_POR_DEFECTO.to_vec(),
            guardar_historial: true,
        }
    }
}

impl Config {
    /// Descarta un modelo que ya no está en el catálogo (por ejemplo `tiny`, que se sacó):
    /// en ese caso se usa el recomendado.
    fn validar(mut self) -> Self {
        if self
            .modelo
            .as_deref()
            .is_some_and(|m| models::find(m).is_err())
        {
            self.modelo = None;
        }
        if !hotkey::atajo_valido(&self.atajo) {
            self.atajo = hotkey::ATAJO_POR_DEFECTO.to_vec();
        }
        self
    }
}

fn ruta(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_config_dir()
        .context("no se encontró la carpeta de configuración")?
        .join("config.json"))
}

/// Lee la configuración. Si no existe o está rota, usa la de por defecto: la app
/// tiene que arrancar igual.
pub fn cargar(app: &AppHandle) -> Config {
    let leida = ruta(app).and_then(|p| {
        let texto = std::fs::read_to_string(&p)?;
        Ok(serde_json::from_str(&texto)?)
    });
    leida
        .unwrap_or_else(|e| {
            if !e.chain().any(|c| {
                c.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
            }) {
                log::warn!("configuración inválida, se usa la de por defecto: {e:#}");
            }
            Config::default()
        })
        .validar()
}

fn guardar(app: &AppHandle, config: &Config) -> Result<()> {
    let p = ruta(app)?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("no se pudo crear {}", dir.display()))?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(config)?)
        .with_context(|| format!("no se pudo guardar {}", p.display()))
}

/// La configuración vigente, compartida entre los comandos.
///
/// Se registra en el `setup`, pero Tauri crea las ventanas de `tauri.conf.json` antes:
/// en release la UI carga tan rápido que puede llamar a los comandos antes de que
/// exista. Por eso los comandos usan `try_state` y no `State`.
pub struct ConfigActual(pub Mutex<Config>);

#[tauri::command]
pub fn obtener_config(app: AppHandle) -> Config {
    app.try_state::<ConfigActual>()
        .and_then(|actual| actual.0.lock().ok().map(|c| c.clone()))
        .unwrap_or_else(|| cargar(&app))
}

#[tauri::command]
pub fn guardar_config(app: AppHandle, config: Config) -> Result<(), String> {
    if !hotkey::atajo_valido(&config.atajo) {
        return Err("ese atajo no se puede usar: elegí Ctrl, Shift, Alt o teclas F".into());
    }
    guardar(&app, &config).map_err(|e| format!("{e:#}"))?;
    #[cfg(windows)]
    hotkey::cambiar_teclas(&config.atajo);
    // Si Wister todavía está arrancando, el hilo de dictado va a leer el archivo recién guardado.
    if let Some(actual) = app.try_state::<ConfigActual>() {
        if let Ok(mut c) = actual.0.lock() {
            *c = config.clone();
        }
    }
    // La ventana se entera de los cambios que vienen de la bandeja (el micrófono), y
    // la bandeja, de los que vienen de la ventana.
    let _ = app.emit("config", &config);
    crate::bandeja::actualizar(&app);
    if let Some(canal) = app.try_state::<CanalDictado>() {
        let _ = canal.0.send(Mensaje::Config(config));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ModeloInfo {
    nombre: &'static str,
    mb: u32,
    nota: &'static str,
    descargado: bool,
    recomendado: bool,
}

#[tauri::command]
pub fn listar_modelos() -> Vec<ModeloInfo> {
    let recomendado = dictado::modelo_por_defecto();
    models::CATALOG
        .iter()
        .map(|m| ModeloInfo {
            nombre: m.name,
            mb: m.size_mb,
            nota: m.note,
            descargado: m.is_downloaded(),
            recomendado: m.name == recomendado,
        })
        .collect()
}

#[derive(Serialize)]
pub struct Microfono {
    nombre: String,
    predeterminado: bool,
}

#[tauri::command]
pub fn listar_microfonos() -> Result<Vec<Microfono>, String> {
    let dispositivos = audio::list_input_devices().map_err(|e| format!("{e:#}"))?;
    Ok(dispositivos
        .into_iter()
        .map(|d| Microfono {
            nombre: d.name,
            predeterminado: d.is_default,
        })
        .collect())
}

/// Evento `descarga`.
#[derive(Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
enum Descarga {
    Progreso {
        modelo: String,
        bajado: u64,
        total: Option<u64>,
    },
    Lista {
        modelo: String,
    },
    Error {
        modelo: String,
        mensaje: String,
    },
}

/// Modelos que se están descargando, para no bajar dos veces el mismo a la vez.
#[derive(Default)]
pub struct Descargas(Mutex<HashSet<String>>);

/// Arranca la descarga en otro hilo y vuelve enseguida; el avance llega por el evento `descarga`.
#[tauri::command]
pub fn descargar_modelo(
    app: AppHandle,
    nombre: String,
    descargas: State<Descargas>,
) -> Result<(), String> {
    let modelo = models::find(&nombre).map_err(|e| format!("{e:#}"))?;
    if !descargas
        .0
        .lock()
        .map(|mut d| d.insert(nombre.clone()))
        .unwrap_or(false)
    {
        return Ok(());
    }
    let dictado = app.try_state::<CanalDictado>().map(|c| c.0.clone());
    std::thread::Builder::new()
        .name(format!("wister-descarga-{nombre}"))
        .spawn(move || {
            // Un evento cada 150 ms alcanza para la barra de progreso.
            let mut ultimo = Instant::now() - Duration::from_secs(1);
            let resultado = models::download(modelo, |bajado, total| {
                if ultimo.elapsed() >= Duration::from_millis(150) {
                    ultimo = Instant::now();
                    let _ = app.emit(
                        "descarga",
                        Descarga::Progreso {
                            modelo: nombre.clone(),
                            bajado,
                            total,
                        },
                    );
                }
            });
            let evento = match resultado {
                Ok(_) => {
                    if let Some(d) = &dictado {
                        let _ = d.send(Mensaje::ModeloDescargado);
                    }
                    Descarga::Lista {
                        modelo: nombre.clone(),
                    }
                }
                Err(e) => Descarga::Error {
                    modelo: nombre.clone(),
                    mensaje: format!("{e:#}"),
                },
            };
            if let Some(d) = app.try_state::<Descargas>() {
                if let Ok(mut d) = d.0.lock() {
                    d.remove(&nombre);
                }
            }
            let _ = app.emit("descarga", evento);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Abre el micrófono solo para mostrar su nivel (evento `nivel_prueba`), sin grabar.
#[tauri::command]
pub fn probar_microfono(app: AppHandle, nombre: Option<String>) {
    if let Some(canal) = app.try_state::<CanalDictado>() {
        let _ = canal.0.send(Mensaje::ProbarMicrofono(nombre));
    }
}

#[tauri::command]
pub fn detener_prueba_microfono(app: AppHandle) {
    if let Some(canal) = app.try_state::<CanalDictado>() {
        let _ = canal.0.send(Mensaje::DetenerPrueba);
    }
}

/// Mientras la configuración captura un atajo nuevo, el actual no tiene que dictar.
#[tauri::command]
pub fn pausar_atajo(pausado: bool) {
    #[cfg(windows)]
    hotkey::pausar(pausado);
    #[cfg(not(windows))]
    let _ = pausado;
}

/// Si Wister arranca al iniciar sesión (clave `Run` del registro del usuario).
#[tauri::command]
pub fn autoarranque(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cambiar_autoarranque(app: AppHandle, activo: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    if activo {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_vacia_usa_valores_por_defecto() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.idioma, "es");
    }

    #[test]
    fn una_config_anterior_al_asistente_lo_muestra() {
        let vieja = r#"{"modelo": "small", "microfono": null, "idioma": "es"}"#;
        let c: Config = serde_json::from_str(vieja).unwrap();
        assert!(!c.asistente_completo);
        assert_eq!(c.modelo.as_deref(), Some("small"));
    }

    #[test]
    fn una_config_vieja_toma_los_valores_nuevos_por_defecto() {
        let vieja = r#"{"modelo": null, "microfono": null, "idioma": "es"}"#;
        let c: Config = serde_json::from_str(vieja).unwrap();
        assert!(c.mostrar_overlay);
        assert!(!c.sonidos);
        assert_eq!(c.atajo, hotkey::ATAJO_POR_DEFECTO);
        assert!(c.guardar_historial);
    }

    #[test]
    fn un_atajo_invalido_vuelve_al_de_por_defecto() {
        let c = Config {
            atajo: vec![0x41],
            ..Config::default()
        };
        assert_eq!(c.validar().atajo, hotkey::ATAJO_POR_DEFECTO);
    }

    #[test]
    fn un_modelo_que_ya_no_existe_vuelve_al_recomendado() {
        let c = Config {
            modelo: Some("tiny".into()),
            ..Config::default()
        };
        assert_eq!(c.validar().modelo, None);
        let c = Config {
            modelo: Some("small".into()),
            ..Config::default()
        };
        assert_eq!(c.validar().modelo.as_deref(), Some("small"));
    }

    #[test]
    fn config_ida_y_vuelta() {
        let c = Config {
            modelo: Some("small".into()),
            microfono: Some("Blue Yeti".into()),
            idioma: "auto".into(),
            asistente_completo: true,
            mostrar_overlay: false,
            sonidos: true,
            atajo: vec![0xA3],
            guardar_historial: false,
        };
        let texto = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&texto).unwrap(), c);
    }
}
