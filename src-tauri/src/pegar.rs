//! Pegar el texto en la ventana activa: portapapeles + `Ctrl+V`, y después se
//! restaura lo que había en el portapapeles.

// Fuera de Windows todavía no se pega (Fase 3).
#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;

/// Cómo terminó el pegado. Salvo en `Hecho`, el texto queda en el portapapeles
/// para que el usuario lo pegue a mano.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Pegado {
    Hecho,
    /// La ventana activa corre como administrador y Windows (UIPI) no deja mandarle teclas.
    VentanaElevada,
    /// Seguían apretadas teclas modificadoras: un `Ctrl+V` podría haber sido `Ctrl+Shift+V`.
    TeclasApretadas,
    Error {
        mensaje: String,
    },
}

/// Hasta cuánto después de un dictado el siguiente se considera su continuación.
pub const CONTINUACION: std::time::Duration = std::time::Duration::from_secs(60);

/// Si el dictado continúa a otro (misma ventana, hace poco), se separa con un espacio:
/// si no, "Hola." + "Qué tal" quedaría "Hola.Qué tal".
pub fn separar(texto: &str, continua: bool) -> String {
    let empieza_pegado = texto
        .chars()
        .next()
        .is_some_and(|c| c.is_whitespace() || ".,;:!?)]}…".contains(c));
    if continua && !empieza_pegado {
        format!(" {texto}")
    } else {
        texto.to_owned()
    }
}

#[cfg(windows)]
pub use windows::Pegador;

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::time::{Duration, Instant};

    use anyhow::{anyhow, bail, Context, Result};
    use windows_sys::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HWND};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
        GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetForegroundWindow, GetWindowThreadProcessId,
        PeekMessageW, TranslateMessage, HWND_MESSAGE, MSG, PM_REMOVE,
    };

    use super::{separar, Pegado, CONTINUACION};

    /// Último pegado exitoso: a qué ventana y cuándo.
    type Ultimo = Option<(HWND, Instant)>;

    const CF_UNICODETEXT: u32 = 13;
    const VK_V: u16 = 0x56;
    /// Cuánto se espera antes de restaurar el portapapeles: la app destino lee el
    /// portapapeles cuando procesa el `Ctrl+V`, no cuando lo recibe.
    const ESPERA_RESTAURAR: Duration = Duration::from_millis(300);
    const ESPERA_TECLAS: Duration = Duration::from_millis(1500);
    const BOMBEO: Duration = Duration::from_millis(10);

    struct Pedido {
        texto: String,
        respuesta: Sender<Pegado>,
    }

    /// Maneja el portapapeles desde un hilo propio.
    ///
    /// El portapapeles necesita una ventana dueña, y Windows le manda mensajes
    /// sincrónicos (por ejemplo `WM_DESTROYCLIPBOARD` cuando otra app lo vacía): si el
    /// hilo dueño no los atiende, esa otra app se cuelga. Por eso el hilo bombea
    /// mensajes todo el tiempo, incluso mientras espera pedidos.
    pub struct Pegador {
        pedidos: Sender<Pedido>,
    }

    impl Pegador {
        pub fn iniciar() -> Result<Self> {
            let (tx, rx) = mpsc::channel();
            let (listo_tx, listo_rx) = mpsc::channel();
            std::thread::Builder::new()
                .name("wister-portapapeles".into())
                .spawn(move || hilo(rx, listo_tx))
                .context("no se pudo crear el hilo del portapapeles")?;
            listo_rx
                .recv()
                .map_err(|_| anyhow!("el hilo del portapapeles terminó al iniciar"))??;
            Ok(Self { pedidos: tx })
        }

        pub fn pegar(&self, texto: &str) -> Pegado {
            let (tx, rx) = mpsc::channel();
            let pedido = Pedido {
                texto: texto.to_owned(),
                respuesta: tx,
            };
            if self.pedidos.send(pedido).is_err() {
                return error("el hilo del portapapeles no está corriendo");
            }
            rx.recv()
                .unwrap_or_else(|_| error("el hilo del portapapeles se cortó"))
        }
    }

    fn error(mensaje: &str) -> Pegado {
        Pegado::Error {
            mensaje: mensaje.to_owned(),
        }
    }

    fn hilo(pedidos: Receiver<Pedido>, listo: Sender<Result<()>>) {
        let ventana = unsafe { crear_ventana() };
        if ventana.is_null() {
            let _ = listo.send(Err(anyhow!("no se pudo crear la ventana del portapapeles")));
            return;
        }
        let _ = listo.send(Ok(()));
        let mut ultimo: Ultimo = None;
        loop {
            bombear();
            match pedidos.recv_timeout(BOMBEO) {
                Ok(pedido) => atender(ventana, pedido, &mut ultimo),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn atender(ventana: HWND, pedido: Pedido, ultimo: &mut Ultimo) {
        let responder = |p: Pegado| {
            let _ = pedido.respuesta.send(p);
        };

        if !esperar_teclas_sueltas() {
            responder(dejar_en_portapapeles(
                ventana,
                &pedido.texto,
                Pegado::TeclasApretadas,
            ));
            return;
        }
        if ventana_activa_elevada() {
            responder(dejar_en_portapapeles(
                ventana,
                &pedido.texto,
                Pegado::VentanaElevada,
            ));
            return;
        }

        let destino = unsafe { GetForegroundWindow() };
        let continua = ultimo.is_some_and(|(v, t)| v == destino && t.elapsed() < CONTINUACION);
        let texto = separar(&pedido.texto, continua);

        let guardado = match guardar(ventana) {
            Ok(g) => g,
            Err(e) => return responder(error(&format!("{e:#}"))),
        };
        if let Err(e) = poner(ventana, &con_texto(&texto)) {
            return responder(error(&format!("{e:#}")));
        }
        let secuencia = unsafe { GetClipboardSequenceNumber() };
        if let Err(e) = enviar_ctrl_v() {
            return responder(error(&format!("{e:#}")));
        }
        responder(Pegado::Hecho);
        *ultimo = Some((destino, Instant::now()));

        esperar_bombeando(ESPERA_RESTAURAR);
        // Si mientras tanto alguien copió otra cosa, no se la pisa.
        if unsafe { GetClipboardSequenceNumber() } == secuencia {
            let mut guardado = guardado;
            guardado.extend(sin_historial());
            if let Err(e) = poner(ventana, &guardado) {
                eprintln!("no se pudo restaurar el portapapeles: {e:#}");
            }
        }
    }

    fn dejar_en_portapapeles(ventana: HWND, texto: &str, motivo: Pegado) -> Pegado {
        match poner(ventana, &con_texto(texto)) {
            Ok(()) => motivo,
            Err(e) => error(&format!("{e:#}")),
        }
    }

    type Entrada = (u32, Vec<u8>);

    /// El texto en UTF-16, marcado para que no entre al historial de `Win+V`.
    fn con_texto(texto: &str) -> Vec<Entrada> {
        let utf16: Vec<u8> = texto
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut entradas = vec![(CF_UNICODETEXT, utf16)];
        entradas.extend(sin_historial());
        entradas
    }

    /// Formatos que le piden a Windows no guardar el contenido en el historial ni en la nube.
    /// https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats
    fn sin_historial() -> Vec<Entrada> {
        let formato = |nombre: &str| unsafe { RegisterClipboardFormatW(wide(nombre).as_ptr()) };
        vec![
            (
                formato("ExcludeClipboardContentFromMonitorProcessing"),
                vec![0; 4],
            ),
            (
                formato("CanIncludeInClipboardHistory"),
                0u32.to_le_bytes().to_vec(),
            ),
            (
                formato("CanUploadToCloudClipboard"),
                0u32.to_le_bytes().to_vec(),
            ),
        ]
    }

    /// Formatos cuyos datos no son memoria global (son objetos GDI o privados de la app):
    /// no se pueden copiar byte a byte. Windows sintetiza CF_BITMAP a partir de CF_DIB.
    fn es_copiable(formato: u32) -> bool {
        !matches!(
            formato,
            2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x200..=0x3FF
        )
    }

    fn guardar(ventana: HWND) -> Result<Vec<Entrada>> {
        let _abierto = Abierto::abrir(ventana)?;
        let mut entradas = Vec::new();
        let mut formato = 0;
        loop {
            formato = unsafe { EnumClipboardFormats(formato) };
            if formato == 0 {
                break;
            }
            if !es_copiable(formato) {
                continue;
            }
            unsafe {
                let datos = GetClipboardData(formato);
                if datos.is_null() {
                    continue;
                }
                let tam = GlobalSize(datos);
                let puntero = GlobalLock(datos) as *const u8;
                if puntero.is_null() {
                    continue;
                }
                entradas.push((formato, std::slice::from_raw_parts(puntero, tam).to_vec()));
                GlobalUnlock(datos);
            }
        }
        Ok(entradas)
    }

    fn poner(ventana: HWND, entradas: &[Entrada]) -> Result<()> {
        let _abierto = Abierto::abrir(ventana)?;
        unsafe {
            if EmptyClipboard() == 0 {
                bail!("no se pudo vaciar el portapapeles");
            }
            for (formato, bytes) in entradas {
                let memoria = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
                if memoria.is_null() {
                    bail!("no hay memoria para el portapapeles");
                }
                let destino = GlobalLock(memoria) as *mut u8;
                if destino.is_null() {
                    GlobalFree(memoria);
                    continue;
                }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), destino, bytes.len());
                GlobalUnlock(memoria);
                // Si SetClipboardData funciona, la memoria pasa a ser del sistema.
                if SetClipboardData(*formato, memoria as HANDLE).is_null() {
                    GlobalFree(memoria);
                }
            }
        }
        Ok(())
    }

    /// Portapapeles abierto; se cierra al salir del alcance.
    struct Abierto;

    impl Abierto {
        fn abrir(ventana: HWND) -> Result<Self> {
            // Otra app puede tenerlo abierto un instante: se reintenta un rato.
            for _ in 0..30 {
                if unsafe { OpenClipboard(ventana) } != 0 {
                    return Ok(Abierto);
                }
                esperar_bombeando(BOMBEO);
            }
            bail!("otra aplicación tiene el portapapeles abierto")
        }
    }

    impl Drop for Abierto {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }

    fn enviar_ctrl_v() -> Result<()> {
        let tecla = |vk: u16, flags: u32| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let entradas = [
            tecla(VK_CONTROL, 0),
            tecla(VK_V, 0),
            tecla(VK_V, KEYEVENTF_KEYUP),
            tecla(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        let enviadas = unsafe {
            SendInput(
                entradas.len() as u32,
                entradas.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if enviadas as usize != entradas.len() {
            bail!("Windows no aceptó el Ctrl+V");
        }
        Ok(())
    }

    /// Espera a que no quede ningún modificador apretado. Devuelve `false` si no se soltaron a tiempo.
    fn esperar_teclas_sueltas() -> bool {
        let limite = Instant::now() + ESPERA_TECLAS;
        loop {
            let apretada = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
                .iter()
                .any(|&vk| unsafe { GetAsyncKeyState(vk as i32) } < 0);
            if !apretada {
                return true;
            }
            if Instant::now() >= limite {
                return false;
            }
            esperar_bombeando(BOMBEO);
        }
    }

    /// Si la ventana activa corre elevada y Wister no: UIPI bloquearía el `Ctrl+V`
    /// sin avisar (SendInput no informa ese caso).
    fn ventana_activa_elevada() -> bool {
        unsafe {
            let ventana = GetForegroundWindow();
            if ventana.is_null() {
                return false;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(ventana, &mut pid);
            if token_elevado(GetCurrentProcess()) == Some(true) {
                return false;
            }
            let proceso = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if proceso.is_null() {
                return false;
            }
            // Sin permiso para leer el token de un proceso casi siempre es porque está elevado.
            let elevado = token_elevado(proceso).unwrap_or(true);
            CloseHandle(proceso);
            elevado
        }
    }

    unsafe fn token_elevado(proceso: HANDLE) -> Option<bool> {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(proceso, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut elevacion = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut largo = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevacion as *mut _ as *mut c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut largo,
        );
        CloseHandle(token);
        (ok != 0).then_some(elevacion.TokenIsElevated != 0)
    }

    unsafe fn crear_ventana() -> HWND {
        // Ventana "solo mensajes": invisible, no aparece en la barra de tareas.
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("Wister portapapeles").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    }

    fn bombear() {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn esperar_bombeando(duracion: Duration) {
        let limite = Instant::now() + duracion;
        while Instant::now() < limite {
            bombear();
            std::thread::sleep(BOMBEO);
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_dictado_que_continua_a_otro_lleva_espacio() {
        assert_eq!(separar("Qué tal", true), " Qué tal");
    }

    #[test]
    fn el_primer_dictado_no_lleva_espacio() {
        assert_eq!(separar("Hola.", false), "Hola.");
    }

    #[test]
    fn la_puntuacion_inicial_no_se_separa() {
        assert_eq!(separar(", y además", true), ", y además");
        assert_eq!(separar("?", true), "?");
        assert_eq!(separar(" ya separado", true), " ya separado");
    }
}
