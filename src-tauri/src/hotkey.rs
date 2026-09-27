//! Atajo push-to-talk.
//!
//! `Detector` es la máquina de estados, sin nada de Win32, para poder testearla.
//! La alimenta Raw Input, en el submódulo `windows`.
//!
//! Las teclas del atajo nunca se tragan: le siguen llegando a la app activa.
//! Por eso, si mientras el atajo está apretado se toca cualquier otra tecla
//! (`Ctrl+Shift+T`, `Ctrl+Shift+←`...), el dictado se cancela.

// Fuera de Windows todavía no hay nada que alimente al detector (Fase 3).
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use serde::Serialize;

/// Códigos de tecla virtual de Windows, que distinguen izquierda y derecha.
pub mod vk {
    pub const LSHIFT: u32 = 0xA0;
    pub const LCONTROL: u32 = 0xA2;
}

/// Atajo por defecto: `Ctrl` + `Shift` izquierdos.
pub const ATAJO_POR_DEFECTO: [u32; 2] = [vk::LCONTROL, vk::LSHIFT];

/// Teclas que se pueden usar en un atajo: `Ctrl`, `Shift` y `Alt` de cada lado, y
/// F1–F24. `Win` no, porque Raw Input no puede evitar que abra el menú Inicio, y las
/// letras tampoco, porque se escribirían en la app mientras se dicta.
pub fn tecla_permitida(codigo: u32) -> bool {
    matches!(codigo, 0xA0..=0xA5 | 0x70..=0x87)
}

pub fn atajo_valido(teclas: &[u32]) -> bool {
    let mut unicas = teclas.to_vec();
    unicas.sort_unstable();
    unicas.dedup();
    (1..=4).contains(&teclas.len())
        && unicas.len() == teclas.len()
        && teclas.iter().all(|&t| tecla_permitida(t))
}

/// Por debajo de esto se considera un toque accidental.
pub const DURACION_MINIMA: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Evento {
    /// Se apretaron todas las teclas del atajo: empezar a grabar.
    Inicio,
    /// Se soltó el atajo después de al menos `DURACION_MINIMA`: transcribir.
    Fin { ms: u64 },
    /// Se tocó otra tecla o fue un toque corto: descartar lo grabado.
    Cancelado { motivo: Motivo },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Motivo {
    OtraTecla,
    ToqueCorto,
}

#[derive(Debug)]
enum Estado {
    Reposo,
    Activo {
        desde: Instant,
    },
    /// Cancelado mientras el atajo sigue apretado: esperar a que se suelte.
    Cancelado,
}

#[derive(Debug)]
pub struct Detector {
    teclas: Vec<u32>,
    apretadas: Vec<bool>,
    estado: Estado,
}

impl Detector {
    pub fn new(teclas: &[u32]) -> Self {
        Self {
            teclas: teclas.to_vec(),
            apretadas: vec![false; teclas.len()],
            estado: Estado::Reposo,
        }
    }

    /// Procesa una tecla. `ahora` se pasa desde afuera para poder testear los tiempos.
    pub fn tecla(&mut self, codigo: u32, abajo: bool, ahora: Instant) -> Option<Evento> {
        let Some(i) = self.teclas.iter().position(|&t| t == codigo) else {
            return self.otra_tecla(abajo);
        };
        // Las repeticiones automáticas llegan como más keydown: se ignoran.
        if self.apretadas[i] == abajo {
            return None;
        }
        self.apretadas[i] = abajo;

        match (&self.estado, abajo) {
            (Estado::Reposo, true) if self.apretadas.iter().all(|&a| a) => {
                self.estado = Estado::Activo { desde: ahora };
                Some(Evento::Inicio)
            }
            (Estado::Activo { desde }, false) => {
                let duracion = ahora.saturating_duration_since(*desde);
                self.estado = Estado::Reposo;
                Some(if duracion < DURACION_MINIMA {
                    Evento::Cancelado {
                        motivo: Motivo::ToqueCorto,
                    }
                } else {
                    Evento::Fin {
                        ms: duracion.as_millis() as u64,
                    }
                })
            }
            (Estado::Cancelado, false) => {
                self.estado = Estado::Reposo;
                None
            }
            _ => None,
        }
    }

    fn otra_tecla(&mut self, abajo: bool) -> Option<Evento> {
        if abajo && matches!(self.estado, Estado::Activo { .. }) {
            self.estado = Estado::Cancelado;
            return Some(Evento::Cancelado {
                motivo: Motivo::OtraTecla,
            });
        }
        None
    }
}

#[cfg(windows)]
pub use windows::{cambiar_teclas, escuchar, pausar};

#[cfg(windows)]
mod windows {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    use anyhow::{bail, Context, Result};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
        RAWINPUTHEADER, RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEKEYBOARD,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, TranslateMessage, MSG, RI_KEY_BREAK,
        RI_KEY_E0, WM_INPUT,
    };

    use super::{Detector, Evento};

    struct Atajo {
        detector: Mutex<Detector>,
        enviar: Box<dyn Fn(Evento) + Send + Sync>,
    }

    static ATAJO: OnceLock<Atajo> = OnceLock::new();
    static PAUSADO: AtomicBool = AtomicBool::new(false);

    /// Cambia las teclas del atajo sin reiniciar la escucha.
    pub fn cambiar_teclas(teclas: &[u32]) {
        if let Some(atajo) = ATAJO.get() {
            if let Ok(mut d) = atajo.detector.lock() {
                *d = Detector::new(teclas);
            }
        }
    }

    /// Con el atajo en pausa no se dispara nada (la configuración está capturando uno nuevo).
    pub fn pausar(pausado: bool) {
        PAUSADO.store(pausado, Ordering::Relaxed);
        // Al volver se arranca de cero: lo que se apretó durante la pausa no cuenta.
        if !pausado {
            if let Some(atajo) = ATAJO.get() {
                if let Ok(mut d) = atajo.detector.lock() {
                    let teclas = d.teclas.clone();
                    *d = Detector::new(&teclas);
                }
            }
        }
    }

    /// Escucha el teclado con Raw Input en un hilo propio y le pasa cada evento del
    /// atajo a `enviar`.
    ///
    /// No se usa un hook `WH_KEYBOARD_LL`: los hooks forman una cadena y cualquier
    /// programa con un hook más nuevo que no llame a `CallNextHookEx` deja a los demás
    /// sin teclas. En una PC de prueba, algún programa (NVIDIA App, Steam, ChatGPT...)
    /// se comía `Ctrl` y `Shift` así. Raw Input recibe una copia de todo lo que llega
    /// al sistema y nadie puede interponerse.
    pub fn escuchar(teclas: &[u32], enviar: impl Fn(Evento) + Send + Sync + 'static) -> Result<()> {
        let atajo = Atajo {
            detector: Mutex::new(Detector::new(teclas)),
            enviar: Box::new(enviar),
        };
        if ATAJO.set(atajo).is_err() {
            bail!("el atajo de teclado ya estaba instalado");
        }
        std::thread::Builder::new()
            .name("wister-atajo".into())
            .spawn(|| unsafe {
                if let Err(e) = registrar() {
                    eprintln!("{e:#}");
                    return;
                }
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    if msg.message == WM_INPUT {
                        procesar(msg.lParam as HRAWINPUT);
                    }
                    // DefWindowProc tiene que ver el WM_INPUT para liberar sus datos.
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .context("no se pudo crear el hilo del atajo")?;
        Ok(())
    }

    unsafe fn registrar() -> Result<()> {
        let wide: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // Ventana oculta: RIDEV_INPUTSINK necesita una ventana destino, y con una de
        // "solo mensajes" (HWND_MESSAGE) no siempre llega la entrada.
        let ventana: HWND = CreateWindowExW(
            0,
            wide.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if ventana.is_null() {
            bail!("no se pudo crear la ventana del atajo");
        }
        // Reemplaza el registro de teclado que hace tao (solo con la app en primer
        // plano y para eventos que no usamos); Windows admite uno por proceso.
        let teclado = RAWINPUTDEVICE {
            usUsagePage: 0x01, // Generic Desktop
            usUsage: 0x06,     // Keyboard
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: ventana,
        };
        if RegisterRawInputDevices(&teclado, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            bail!("no se pudo registrar el teclado con Raw Input");
        }
        Ok(())
    }

    unsafe fn procesar(datos: HRAWINPUT) {
        let Some(atajo) = ATAJO.get() else {
            return;
        };
        if PAUSADO.load(Ordering::Relaxed) {
            return;
        }
        let mut entrada: RAWINPUT = std::mem::zeroed();
        let mut tam = std::mem::size_of::<RAWINPUT>() as u32;
        let leidos = GetRawInputData(
            datos,
            RID_INPUT,
            &mut entrada as *mut _ as *mut _,
            &mut tam,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        if leidos == u32::MAX || entrada.header.dwType != RIM_TYPEKEYBOARD {
            return;
        }
        // Las teclas inyectadas con SendInput (por ejemplo, nuestro propio Ctrl+V)
        // llegan sin dispositivo.
        if entrada.header.hDevice.is_null() {
            return;
        }
        let teclado = entrada.data.keyboard;
        let Some(codigo) = tecla_con_lado(teclado.VKey, teclado.MakeCode, teclado.Flags) else {
            return;
        };
        let abajo = teclado.Flags as u32 & RI_KEY_BREAK == 0;
        let evento = atajo
            .detector
            .lock()
            .ok()
            .and_then(|mut d| d.tecla(codigo, abajo, Instant::now()));
        if let Some(evento) = evento {
            (atajo.enviar)(evento);
        }
    }

    /// Raw Input informa `VK_SHIFT`/`VK_CONTROL`/`VK_MENU` sin lado: se deduce del
    /// scan code y del prefijo E0. Devuelve `None` para las teclas "falsas" que genera
    /// el teclado (VKey 0xFF, o Shift con E0 alrededor de las flechas con Bloq Num).
    fn tecla_con_lado(vkey: u16, make_code: u16, flags: u16) -> Option<u32> {
        let e0 = flags as u32 & RI_KEY_E0 != 0;
        Some(match vkey {
            0xFF => return None,
            0x10 if e0 => return None,
            0x10 if make_code == 0x36 => 0xA1, // Shift derecho
            0x10 => 0xA0,                      // Shift izquierdo
            0x11 if e0 => 0xA3,                // Ctrl derecho
            0x11 => 0xA2,                      // Ctrl izquierdo
            0x12 if e0 => 0xA5,                // Alt derecho (AltGr)
            0x12 => 0xA4,                      // Alt izquierdo
            otra => otra as u32,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::tecla_con_lado;

        #[test]
        fn los_modificadores_se_separan_por_lado() {
            assert_eq!(tecla_con_lado(0x10, 0x2A, 0), Some(0xA0));
            assert_eq!(tecla_con_lado(0x10, 0x36, 0), Some(0xA1));
            assert_eq!(tecla_con_lado(0x11, 0x1D, 0), Some(0xA2));
            assert_eq!(tecla_con_lado(0x11, 0x1D, 2), Some(0xA3));
        }

        #[test]
        fn las_teclas_falsas_se_ignoran() {
            assert_eq!(tecla_con_lado(0xFF, 0, 0), None);
            // Shift "falso" con E0 que mandan algunas flechas con Bloq Num.
            assert_eq!(tecla_con_lado(0x10, 0x2A, 2), None);
        }

        #[test]
        fn las_demas_teclas_pasan_igual() {
            assert_eq!(tecla_con_lado(0x41, 0x1E, 0), Some(0x41));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atajos_validos_e_invalidos() {
        assert!(atajo_valido(&ATAJO_POR_DEFECTO));
        assert!(atajo_valido(&[0xA3])); // Ctrl derecho solo
        assert!(atajo_valido(&[0xA4, 0x78])); // Alt + F9
        assert!(!atajo_valido(&[]));
        assert!(!atajo_valido(&[0x41])); // una letra
        assert!(!atajo_valido(&[0xA2, 0x5B])); // Ctrl + Win
        assert!(!atajo_valido(&[0xA2, 0xA2])); // repetida
    }

    const A: u32 = 0x41;

    struct Prueba {
        detector: Detector,
        t0: Instant,
    }

    impl Prueba {
        fn new() -> Self {
            Self {
                detector: Detector::new(&ATAJO_POR_DEFECTO),
                t0: Instant::now(),
            }
        }
        fn tecla(&mut self, codigo: u32, abajo: bool, ms: u64) -> Option<Evento> {
            let ahora = self.t0 + Duration::from_millis(ms);
            self.detector.tecla(codigo, abajo, ahora)
        }
    }

    #[test]
    fn apretar_y_soltar_el_atajo_dicta() {
        let mut p = Prueba::new();
        assert_eq!(p.tecla(vk::LCONTROL, true, 0), None);
        assert_eq!(p.tecla(vk::LSHIFT, true, 10), Some(Evento::Inicio));
        assert_eq!(
            p.tecla(vk::LSHIFT, false, 1510),
            Some(Evento::Fin { ms: 1500 })
        );
        assert_eq!(p.tecla(vk::LCONTROL, false, 1520), None);
    }

    #[test]
    fn el_orden_de_las_teclas_no_importa() {
        let mut p = Prueba::new();
        p.tecla(vk::LSHIFT, true, 0);
        assert_eq!(p.tecla(vk::LCONTROL, true, 5), Some(Evento::Inicio));
    }

    #[test]
    fn las_repeticiones_automaticas_se_ignoran() {
        let mut p = Prueba::new();
        p.tecla(vk::LCONTROL, true, 0);
        assert_eq!(p.tecla(vk::LSHIFT, true, 0), Some(Evento::Inicio));
        assert_eq!(p.tecla(vk::LSHIFT, true, 500), None);
        assert_eq!(p.tecla(vk::LCONTROL, true, 530), None);
        assert_eq!(
            p.tecla(vk::LCONTROL, false, 800),
            Some(Evento::Fin { ms: 800 })
        );
    }

    #[test]
    fn otra_tecla_cancela_y_no_vuelve_a_disparar_hasta_soltar() {
        let mut p = Prueba::new();
        p.tecla(vk::LCONTROL, true, 0);
        p.tecla(vk::LSHIFT, true, 0);
        assert_eq!(
            p.tecla(A, true, 400),
            Some(Evento::Cancelado {
                motivo: Motivo::OtraTecla
            })
        );
        assert_eq!(p.tecla(A, false, 450), None);
        assert_eq!(p.tecla(vk::LSHIFT, false, 900), None);
        // Con Ctrl todavía apretado, volver a apretar Shift arranca de nuevo.
        assert_eq!(p.tecla(vk::LSHIFT, true, 1000), Some(Evento::Inicio));
    }

    #[test]
    fn un_toque_corto_se_descarta() {
        let mut p = Prueba::new();
        p.tecla(vk::LCONTROL, true, 0);
        p.tecla(vk::LSHIFT, true, 0);
        assert_eq!(
            p.tecla(vk::LSHIFT, false, 120),
            Some(Evento::Cancelado {
                motivo: Motivo::ToqueCorto
            })
        );
    }

    #[test]
    fn las_teclas_derechas_no_disparan() {
        const RCONTROL: u32 = 0xA3;
        let mut p = Prueba::new();
        p.tecla(RCONTROL, true, 0);
        assert_eq!(p.tecla(vk::LSHIFT, true, 0), None);
    }
}
