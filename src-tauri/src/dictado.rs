//! Hilo de dictado: carga el modelo, graba mientras el atajo está apretado y
//! transcribe al soltarlo. También atiende los cambios de configuración.
//!
//! Todo pasa en un solo hilo, así que la grabación (`cpal::Stream`) nunca cambia de hilo.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use wister_core::audio::{self, Audio, Recording};
use wister_core::vad::Vad;
use wister_core::{models, stt};

use crate::config::{Config, ConfigActual};
use crate::historial;
use crate::hotkey::{Evento, Motivo, DURACION_MINIMA};
use crate::overlay;
use crate::pegar::Pegado;
use crate::rendimiento;
use crate::sistema::Sistema;
use crate::sonidos::{self, Sonido};

/// Lo que le llega al hilo de dictado.
#[derive(Debug)]
pub enum Mensaje {
    // Fuera de Windows todavía no hay atajo global.
    #[cfg_attr(not(windows), allow(dead_code))]
    Atajo(Evento),
    Config(Config),
    /// Terminó una descarga: si no había modelo cargado, se reintenta.
    ModeloDescargado,
    /// Abrir un micrófono solo para mostrar su nivel en la configuración.
    ProbarMicrofono(Option<String>),
    DetenerPrueba,
    /// Prueba de rendimiento: grabar la frase (con el micrófono de la configuración)...
    GrabarRendimiento {
        frase: &'static str,
        idioma: &'static str,
    },
    /// ...y medir cada modelo con lo grabado.
    MedirRendimiento,
    CancelarRendimiento,
}

/// Para mandarle mensajes al hilo de dictado desde los comandos de la UI.
pub struct CanalDictado(pub Sender<Mensaje>);

/// Estado de la app, que la UI pide al abrirse y después sigue por el evento `estado`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Estado {
    #[default]
    Iniciando,
    CargandoModelo {
        modelo: String,
    },
    Listo {
        modelo: String,
        gpu: bool,
    },
    Grabando,
    Transcribiendo,
    Error {
        mensaje: String,
    },
}

/// Resultado de cada dictado, que se manda por el evento `resultado`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Resultado {
    Texto {
        texto: String,
        audio_ms: u64,
        /// Desde que se soltó el atajo hasta tener el texto.
        espera_ms: u64,
        /// Lo que tardó en abrirse el micrófono al apretar el atajo.
        microfono_ms: u64,
        pegado: Pegado,
        /// Título de la ventana activa al soltar el atajo.
        app: Option<String>,
    },
    Descartado {
        motivo: Descarte,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Descarte {
    OtraTecla,
    ToqueCorto,
    /// Sin voz según el filtro de energía o el VAD, o una frase que Whisper inventó.
    SinVoz,
}

#[derive(Default)]
pub struct EstadoActual(pub Mutex<Estado>);

#[tauri::command]
pub fn estado_actual(estado: tauri::State<EstadoActual>) -> Estado {
    estado.0.lock().map(|e| e.clone()).unwrap_or_default()
}

/// Modelo por defecto según los resultados de la Fase 0 (ver docs/fase-0.md).
pub fn modelo_por_defecto() -> &'static str {
    if stt::gpu_available() {
        "large-v3-turbo-q5_0"
    } else {
        "small"
    }
}

pub fn iniciar(app: AppHandle, mensajes: Receiver<Mensaje>, config: Config) -> Result<()> {
    std::thread::Builder::new()
        .name("wister-dictado".into())
        .spawn(move || {
            stt::silence_native_logs();
            // Acá y no en el `setup`: listar las GPU inicializa Vulkan, y eso tarda.
            log::info!("{}", Sistema::detectar().resumen());
            let mut dictado = match Dictado::new(&app, config) {
                Ok(d) => d,
                Err(e) => return publicar_error(&app, &e),
            };
            dictado.cargar_modelo();
            // Lo que se apretó mientras cargaba el modelo ya no tiene sentido.
            let pendientes: Vec<_> = mensajes.try_iter().collect();
            for m in pendientes {
                if !matches!(m, Mensaje::Atajo(_)) {
                    dictado.recibir(m);
                }
            }
            loop {
                // Mientras hay una grabación sin confirmar, se espera con plazo: si vence
                // sin que se suelte el atajo ni se toque otra tecla, se confirma.
                let mensaje = match dictado.confirmar_en {
                    Some(plazo) => {
                        match mensajes.recv_timeout(plazo.saturating_duration_since(Instant::now()))
                        {
                            Ok(m) => m,
                            Err(RecvTimeoutError::Timeout) => {
                                dictado.confirmar();
                                continue;
                            }
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    None => match mensajes.recv() {
                        Ok(m) => m,
                        Err(_) => return,
                    },
                };
                dictado.recibir(mensaje);
            }
        })
        .context("no se pudo crear el hilo de dictado")?;
    Ok(())
}

struct Grabacion {
    recording: Recording,
    microfono_ms: u64,
    /// Si ya se mostró (overlay, sonido). Ver `Dictado::confirmar`.
    confirmada: bool,
}

struct GrabacionRendimiento {
    recording: Recording,
    frase: &'static str,
    idioma: &'static str,
}

struct Modelo {
    nombre: String,
    engine: stt::Engine,
}

struct Dictado {
    app: AppHandle,
    config: Config,
    /// Silero VAD; si no se pudo cargar, queda solo el filtro por energía.
    vad: Option<Vad>,
    modelo: Option<Modelo>,
    /// Estado al que se vuelve después de cada dictado (`Listo`, o el error de carga).
    reposo: Estado,
    grabacion: Option<Grabacion>,
    /// Cuándo confirmar la grabación en curso.
    confirmar_en: Option<Instant>,
    /// Micrófono abierto para el medidor de nivel de la configuración.
    prueba: Option<Recording>,
    /// Frase que se está grabando para la prueba de rendimiento.
    rendimiento: Option<GrabacionRendimiento>,
    /// Último micrófono anotado en el registro, para anotarlo solo cuando cambia.
    ultimo_microfono: String,
    #[cfg(windows)]
    pegador: crate::pegar::Pegador,
}

impl Dictado {
    fn new(app: &AppHandle, config: Config) -> Result<Self> {
        let vad = Vad::load()
            .map_err(|e| log::warn!("sin VAD, solo el filtro por energía: {e:#}"))
            .ok();
        Ok(Self {
            app: app.clone(),
            config,
            vad,
            modelo: None,
            reposo: Estado::Iniciando,
            grabacion: None,
            confirmar_en: None,
            prueba: None,
            rendimiento: None,
            ultimo_microfono: String::new(),
            #[cfg(windows)]
            pegador: crate::pegar::Pegador::iniciar()?,
        })
    }

    fn recibir(&mut self, mensaje: Mensaje) {
        match mensaje {
            Mensaje::Atajo(evento) => self.atender(evento),
            Mensaje::Config(config) => {
                // El idioma y el micrófono se leen en cada dictado; el modelo solo se
                // recarga si cambió.
                self.config = config;
                self.cargar_modelo();
            }
            Mensaje::ModeloDescargado => {
                if self.modelo.is_none() {
                    self.cargar_modelo();
                }
            }
            Mensaje::ProbarMicrofono(nombre) => {
                self.detener_prueba();
                let app = self.app.clone();
                let nivel = move |rms: f32| {
                    let _ = app.emit_to("config", "nivel_prueba", rms);
                };
                match audio::start_recording_with_levels(nombre.as_deref(), nivel) {
                    Ok(r) => self.prueba = Some(r),
                    Err(e) => publicar_error(&self.app, &e),
                }
            }
            Mensaje::DetenerPrueba => self.detener_prueba(),
            Mensaje::GrabarRendimiento { frase, idioma } => {
                self.cancelar_rendimiento();
                if self.grabacion.is_some() {
                    return;
                }
                self.detener_prueba();
                match self.abrir_microfono("config", "nivel_prueba") {
                    Ok(recording) => {
                        self.rendimiento = Some(GrabacionRendimiento {
                            recording,
                            frase,
                            idioma,
                        })
                    }
                    Err(e) => emitir_rendimiento(
                        &self.app,
                        rendimiento::Evento::Error {
                            mensaje: format!("{e:#}"),
                        },
                    ),
                }
            }
            Mensaje::MedirRendimiento => self.medir_rendimiento(),
            Mensaje::CancelarRendimiento => self.cancelar_rendimiento(),
        }
    }

    fn cancelar_rendimiento(&mut self) {
        if let Some(r) = self.rendimiento.take() {
            r.recording.stop();
        }
    }

    fn medir_rendimiento(&mut self) {
        let Some(grabacion) = self.rendimiento.take() else {
            return;
        };
        let audio = grabacion.recording.stop();
        let voz = match preparar_rendimiento(audio, self.vad.as_mut()) {
            Ok(v) => v,
            Err(e) => {
                return emitir_rendimiento(
                    &self.app,
                    rendimiento::Evento::Error {
                        mensaje: format!("{e:#}"),
                    },
                )
            }
        };
        log::info!(
            "prueba de rendimiento con {:.1} s de voz",
            voz.duration_secs()
        );
        // Mientras se mide no se dicta: cada modelo se carga y se libera, y el de la
        // configuración se libera antes para no ocupar memoria de más.
        #[cfg(windows)]
        crate::hotkey::pausar(true);
        self.modelo = None;
        let app = self.app.clone();
        let mediciones = rendimiento::medir(&voz, grabacion.frase, grabacion.idioma, |e| {
            emitir_rendimiento(&app, e)
        });
        emitir_rendimiento(
            &self.app,
            rendimiento::Evento::Fin {
                voz_ms: (voz.duration_secs() * 1000.0) as u64,
                recomendado: rendimiento::recomendar(&mediciones, stt::gpu_available()),
            },
        );
        self.cargar_modelo();
        #[cfg(windows)]
        crate::hotkey::pausar(false);
    }

    fn detener_prueba(&mut self) {
        // `stop` además termina el hilo que junta las muestras: no alcanza con soltarla.
        if let Some(r) = self.prueba.take() {
            r.stop();
        }
    }

    fn nombre_modelo(&self) -> String {
        self.config
            .modelo
            .clone()
            .unwrap_or_else(|| modelo_por_defecto().to_owned())
    }

    fn opciones(&self) -> stt::Options {
        stt::Options {
            language: self.config.idioma.clone(),
            ..Default::default()
        }
    }

    /// Carga el modelo de la configuración, salvo que ya sea el cargado.
    fn cargar_modelo(&mut self) {
        let nombre = self.nombre_modelo();
        if self.modelo.as_ref().is_some_and(|m| m.nombre == nombre) {
            return;
        }
        // Se libera el anterior antes de cargar: si no, por un momento ocupan memoria los dos.
        self.modelo = None;
        publicar(
            &self.app,
            Estado::CargandoModelo {
                modelo: nombre.clone(),
            },
        );
        match cargar_engine(&nombre, &self.opciones(), true) {
            Ok(engine) => {
                self.reposo = Estado::Listo {
                    modelo: nombre.clone(),
                    gpu: engine.gpu,
                };
                self.modelo = Some(Modelo { nombre, engine });
            }
            Err(e) => {
                log::error!("{e:#}");
                self.reposo = Estado::Error {
                    mensaje: format!("{e:#}"),
                };
            }
        }
        publicar(&self.app, self.reposo.clone());
    }

    fn atender(&mut self, evento: Evento) {
        if !matches!(evento, Evento::Inicio) {
            self.confirmar_en = None;
        }
        let resultado = match evento {
            Evento::Inicio => {
                self.grabar();
                return;
            }
            Evento::Cancelado { motivo } => {
                // Se descarta lo grabado sin transcribir. Si ni se había confirmado, fue
                // un atajo común o un toque: no vale la pena anotarlo en el historial.
                let confirmada = self.grabacion.take().is_some_and(|g| {
                    g.recording.stop();
                    g.confirmada
                });
                Ok(confirmada.then_some(Resultado::Descartado {
                    motivo: match motivo {
                        Motivo::OtraTecla => Descarte::OtraTecla,
                        Motivo::ToqueCorto => Descarte::ToqueCorto,
                    },
                }))
            }
            Evento::Fin { .. } => self.transcribir(),
        };
        match resultado {
            Ok(Some(r)) => {
                anotar(&r);
                let _ = self.app.emit("resultado", &r);
                publicar(&self.app, self.reposo.clone());
                // Después de ocultar el overlay: guardar puede tardar (la primera vez abre la base).
                if let Resultado::Texto {
                    texto,
                    audio_ms,
                    app,
                    ..
                } = &r
                {
                    if self.config.guardar_historial {
                        historial::anotar(&self.app, texto, *audio_ms, app.as_deref());
                    }
                }
            }
            Ok(None) => publicar(&self.app, self.reposo.clone()),
            Err(e) => publicar_error(&self.app, &e),
        }
    }

    fn grabar(&mut self) {
        if self.grabacion.is_some() {
            return;
        }
        // El dictado tiene prioridad sobre el medidor y la prueba de la configuración.
        self.detener_prueba();
        if self.rendimiento.is_some() {
            self.cancelar_rendimiento();
            emitir_rendimiento(
                &self.app,
                rendimiento::Evento::Error {
                    mensaje: "Se canceló la prueba porque empezaste a dictar.".into(),
                },
            );
        }
        let inicio = Instant::now();
        let recording = match self.abrir_microfono("overlay", "nivel") {
            Ok(r) => r,
            Err(e) => return publicar_error(&self.app, &e),
        };
        self.grabacion = Some(Grabacion {
            recording,
            microfono_ms: inicio.elapsed().as_millis() as u64,
            confirmada: false,
        });
        // Se graba desde ya para no perder el principio, pero el overlay y el sonido
        // esperan: si en ese lapso se toca otra tecla es un atajo común (Ctrl+Shift+T)
        // y no tiene que aparecer nada.
        self.confirmar_en = Some(inicio + DURACION_MINIMA);
    }

    fn confirmar(&mut self) {
        self.confirmar_en = None;
        let Some(g) = self.grabacion.as_mut() else {
            return;
        };
        g.confirmada = true;
        publicar(&self.app, Estado::Grabando);
        if self.config.sonidos {
            sonidos::reproducir(Sonido::Inicio);
        }
    }

    /// Abre el micrófono de la configuración y manda el nivel a `ventana` por `evento`.
    fn abrir_microfono(
        &mut self,
        ventana: &'static str,
        evento: &'static str,
    ) -> Result<Recording> {
        let nivel = |app: AppHandle| {
            move |rms: f32| {
                let _ = app.emit_to(ventana, evento, rms);
            }
        };
        let elegido = self.config.microfono.as_deref();
        let recording = match audio::start_recording_with_levels(elegido, nivel(self.app.clone())) {
            // Si el micrófono elegido no está (se desconectó el USB, por ejemplo), se usa
            // el predeterminado antes que no dictar.
            Err(e) if elegido.is_some() => {
                log::warn!("{e:#}; se usa el micrófono predeterminado");
                audio::start_recording_with_levels(None, nivel(self.app.clone()))
            }
            r => r,
        }?;
        if recording.device_name != self.ultimo_microfono {
            log::info!("micrófono: {}", recording.device_name);
            self.ultimo_microfono = recording.device_name.clone();
        }
        Ok(recording)
    }

    fn transcribir(&mut self) -> Result<Option<Resultado>> {
        let Some(grabacion) = self.grabacion.take() else {
            return Ok(None);
        };
        let audio = grabacion.recording.stop();
        if grabacion.confirmada && self.config.sonidos {
            sonidos::reproducir(Sonido::Fin);
        }
        let soltado = Instant::now();
        // Se toma ahora: mientras Whisper transcribe el usuario puede cambiar de ventana.
        let app = ventana_activa();
        if !audio.has_voice() {
            return Ok(Some(Resultado::Descartado {
                motivo: Descarte::SinVoz,
            }));
        }
        let opciones = self.opciones();
        let Some(modelo) = self.modelo.as_mut() else {
            bail!("no hay un modelo cargado: elegí o descargá uno en Configuración");
        };
        publicar(&self.app, Estado::Transcribiendo);
        let audio_ms = (audio.duration_secs() * 1000.0) as u64;
        let mut a16k = audio.to_whisper()?;
        if let Some(vad) = self.vad.as_mut() {
            let voz = vad.speech(&a16k)?;
            if voz.samples.is_empty() {
                return Ok(Some(Resultado::Descartado {
                    motivo: Descarte::SinVoz,
                }));
            }
            a16k = voz;
        }
        let remuestreo = soltado.elapsed();
        let transcript = modelo.engine.transcribe(&a16k, &opciones)?;
        log::info!(
            "remuestreo y VAD {} ms + whisper {} ms",
            remuestreo.as_millis(),
            transcript.elapsed.as_millis()
        );
        if transcript.text.is_empty() {
            return Ok(Some(Resultado::Descartado {
                motivo: Descarte::SinVoz,
            }));
        }
        if stt::is_hallucination(&transcript.text) {
            log::info!("frase fantasma descartada: {:?}", transcript.text);
            return Ok(Some(Resultado::Descartado {
                motivo: Descarte::SinVoz,
            }));
        }
        let espera_ms = soltado.elapsed().as_millis() as u64;
        let pegado = self.pegar(&transcript.text);
        Ok(Some(Resultado::Texto {
            texto: transcript.text,
            audio_ms,
            espera_ms,
            microfono_ms: grabacion.microfono_ms,
            pegado,
            app,
        }))
    }

    #[cfg(windows)]
    fn pegar(&self, texto: &str) -> Pegado {
        self.pegador.pegar(texto)
    }

    #[cfg(not(windows))]
    fn pegar(&self, _texto: &str) -> Pegado {
        Pegado::Error {
            mensaje: "pegar todavía solo funciona en Windows".into(),
        }
    }
}

#[cfg(windows)]
fn ventana_activa() -> Option<String> {
    crate::pegar::titulo_ventana_activa()
}

#[cfg(not(windows))]
fn ventana_activa() -> Option<String> {
    None
}

/// Carga un modelo y lo "calienta". Con `gpu` en `false` usa la CPU aunque haya GPU.
pub fn cargar_engine(nombre: &str, opciones: &stt::Options, gpu: bool) -> Result<stt::Engine> {
    let modelo = models::find(nombre)?;
    if !modelo.is_downloaded() {
        bail!("falta el modelo {nombre}: descargalo en Configuración");
    }
    let ruta = modelo.path()?;
    let mut engine = crate::registro::con_detalle_nativo(|| stt::Engine::load(&ruta, gpu))?;

    // La primera transcripción es lenta (con Vulkan se compilan los shaders: ~7 s en
    // una RTX 5070 Ti). Se hace una de prueba ahora para que no la pague el primer dictado.
    let silencio = Audio {
        samples: vec![0.0; audio::WHISPER_SAMPLE_RATE as usize],
        sample_rate: audio::WHISPER_SAMPLE_RATE,
    };
    let calentamiento = engine.transcribe(&silencio, opciones)?;
    log::info!(
        "modelo {nombre} cargado en {} ms ({}), calentamiento {} ms",
        engine.load_time.as_millis(),
        if engine.gpu { "GPU" } else { "CPU" },
        calentamiento.elapsed.as_millis()
    );
    Ok(engine)
}

/// Anota el resultado de un dictado en el registro, sin el texto.
fn anotar(r: &Resultado) {
    match r {
        Resultado::Texto {
            texto,
            audio_ms,
            espera_ms,
            microfono_ms,
            pegado,
            ..
        } => log::info!(
            "dictado: {audio_ms} ms de audio, {} palabras, micrófono {microfono_ms} ms, espera {espera_ms} ms, pegado: {pegado:?}",
            texto.split_whitespace().count()
        ),
        Resultado::Descartado { motivo } => log::info!("dictado descartado: {motivo:?}"),
    }
}

/// Recorta la frase de la prueba de rendimiento con el VAD y la deja a 16 kHz.
fn preparar_rendimiento(audio: Audio, vad: Option<&mut Vad>) -> Result<Audio> {
    if audio.duration_secs() < 3.0 {
        bail!("Grabaste muy poco: leé la frase completa y después tocá \"Terminé\".");
    }
    let mut voz = audio.to_whisper()?;
    if let Some(vad) = vad {
        voz = vad.speech(&voz)?;
    }
    if voz.samples.is_empty() {
        bail!("No se detectó voz. Fijate que el micrófono esté bien elegido y no esté silenciado.");
    }
    Ok(voz)
}

fn emitir_rendimiento(app: &AppHandle, evento: rendimiento::Evento) {
    let _ = app.emit_to("config", "rendimiento", evento);
}

fn publicar_error(app: &AppHandle, e: &anyhow::Error) {
    log::error!("{e:#}");
    publicar(
        app,
        Estado::Error {
            mensaje: format!("{e:#}"),
        },
    );
}

fn publicar(app: &AppHandle, estado: Estado) {
    if let Some(actual) = app.try_state::<EstadoActual>() {
        if let Ok(mut e) = actual.0.lock() {
            *e = estado.clone();
        }
    }
    let _ = app.emit("estado", &estado);
    let con_overlay = app
        .try_state::<ConfigActual>()
        .and_then(|c| c.0.lock().ok().map(|c| c.mostrar_overlay))
        .unwrap_or(true);
    match estado {
        Estado::Grabando if con_overlay => overlay::mostrar(app),
        Estado::Transcribiendo => {}
        _ => overlay::ocultar(app),
    }
}
