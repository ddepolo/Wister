//! Overlay: la pastilla con la forma de onda que se ve mientras se graba, y con
//! avisos cortos ("Copiado al portapapeles").
//!
//! No puede quedarse con el foco nunca: el `Ctrl+V` tiene que llegarle a la app
//! donde el usuario está escribiendo. Por eso en Windows lleva `WS_EX_NOACTIVATE` y
//! se muestra con `SW_SHOWNOACTIVATE` (el `show()` de Tauri activa la ventana).

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use crate::dictado::{Estado, EstadoActual};

const VENTANA: &str = "overlay";
/// Separación entre la pastilla y el borde de abajo del área de trabajo (sobre la barra de tareas).
const MARGEN_INFERIOR: f64 = 24.0;

fn ventana(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(VENTANA)
}

/// Ajusta la ventana una vez, al arrancar.
pub fn preparar(app: &AppHandle) -> tauri::Result<()> {
    let Some(v) = ventana(app) else {
        return Ok(());
    };
    // Los clics pasan a lo que haya debajo.
    v.set_ignore_cursor_events(true)?;
    #[cfg(windows)]
    windows::sin_activar(&v)?;
    Ok(())
}

pub fn mostrar(app: &AppHandle) {
    let Some(v) = ventana(app) else {
        return;
    };
    let _ = posicionar(app, &v);
    #[cfg(windows)]
    windows::mostrar(&v);
    #[cfg(not(windows))]
    let _ = v.show();
}

/// Muestra `texto` en la pastilla por un momento.
pub fn avisar(app: &AppHandle, texto: &str) {
    let _ = app.emit_to(VENTANA, "aviso", texto);
    mostrar(app);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        // Si mientras tanto se empezó a dictar, la pastilla ya es la de la grabación.
        let dictando = app.try_state::<EstadoActual>().is_some_and(|e| {
            e.0.lock()
                .is_ok_and(|e| matches!(*e, Estado::Grabando | Estado::Transcribiendo))
        });
        if !dictando {
            ocultar(&app);
        }
    });
}

pub fn ocultar(app: &AppHandle) {
    let Some(v) = ventana(app) else {
        return;
    };
    // Tiene que ser simétrico con `mostrar`: como Tauri no se enteró de que la ventana
    // se mostró, su `hide()` no hace nada.
    #[cfg(windows)]
    windows::ocultar(&v);
    #[cfg(not(windows))]
    let _ = v.hide();
}

/// Abajo al centro del monitor donde está el mouse, que suele ser donde se está escribiendo.
fn posicionar(app: &AppHandle, v: &WebviewWindow) -> tauri::Result<()> {
    let cursor = app.cursor_position()?;
    let monitor = match app.monitor_from_point(cursor.x, cursor.y)? {
        Some(m) => m,
        None => match app.primary_monitor()? {
            Some(m) => m,
            None => return Ok(()),
        },
    };
    let area = monitor.work_area();
    let tam = v.outer_size()?;
    let margen = (MARGEN_INFERIOR * monitor.scale_factor()) as i32;
    let x = area.position.x + (area.size.width as i32 - tam.width as i32) / 2;
    let y = area.position.y + area.size.height as i32 - tam.height as i32 - margen;
    v.set_position(PhysicalPosition::new(x, y))
}

#[cfg(windows)]
mod windows {
    use tauri::WebviewWindow;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };

    fn hwnd(v: &WebviewWindow) -> tauri::Result<HWND> {
        Ok(v.hwnd()?.0 as HWND)
    }

    pub fn sin_activar(v: &WebviewWindow) -> tauri::Result<()> {
        let h = hwnd(v)?;
        unsafe {
            let estilo = GetWindowLongPtrW(h, GWL_EXSTYLE);
            // TOOLWINDOW: tampoco aparece en Alt+Tab.
            SetWindowLongPtrW(
                h,
                GWL_EXSTYLE,
                estilo | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
            );
        }
        Ok(())
    }

    pub fn mostrar(v: &WebviewWindow) {
        let Ok(h) = hwnd(v) else {
            return;
        };
        unsafe {
            ShowWindow(h, SW_SHOWNOACTIVATE);
            SetWindowPos(
                h,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    pub fn ocultar(v: &WebviewWindow) {
        if let Ok(h) = hwnd(v) {
            unsafe { ShowWindow(h, SW_HIDE) };
        }
    }
}
