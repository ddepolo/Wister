//! Placa de video: si se usa y qué pasa cuando no se puede.
//!
//! Con Vulkan, el ejecutable carga `vulkan-1.dll` (viene con los drivers de video) recién
//! cuando whisper.cpp lo usa (`/DELAYLOAD`, ver `build.rs`). Así el mismo instalador
//! abre en una PC sin drivers o en una máquina virtual: si la DLL no está, los hooks de
//! acá le dan a whisper.cpp un Vulkan que falla al iniciarse, y sigue con la CPU como
//! cuando no hay GPU compatible.
//!
//! Lo mismo se hace a propósito cuando el usuario apagó "Usar la placa de video" o
//! cuando la última vez Wister se cerró de golpe mientras usaba la GPU (un driver con
//! problemas). Eso se detecta con un archivo que existe solo mientras se inicia Vulkan
//! o se carga un modelo en la GPU: si al arrancar sigue ahí, la app no terminó bien.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Manager, Wry};

/// Vulkan no se carga en esta sesión: whisper.cpp usa la CPU.
static BLOQUEADA: AtomicBool = AtomicBool::new(false);
/// La sesión anterior se cerró mientras usaba la GPU.
static FALLO: AtomicBool = AtomicBool::new(false);
/// Falta `vulkan-1.dll`: no hay drivers de video con Vulkan.
static SIN_VULKAN: AtomicBool = AtomicBool::new(false);
static MARCA: OnceLock<PathBuf> = OnceLock::new();

/// Decide si se carga Vulkan. Es un plugin porque los plugins se inician antes de crear
/// las ventanas (que llaman comandos que pueden tocar Vulkan) y después del de instancia
/// única: una segunda instancia se cierra antes de llegar acá y no toca la marca.
pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new("wister-gpu")
        .setup(|app, _| {
            if cfg!(feature = "vulkan") {
                preparar(app);
            }
            Ok(())
        })
        .build()
}

fn preparar(app: &AppHandle) {
    let Ok(carpeta) = app.path().app_config_dir() else {
        return;
    };
    let marca = carpeta.join("usando-gpu");
    let mut config = crate::config::cargar(app);
    if marca.exists() {
        FALLO.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&marca);
        if config.usar_gpu {
            config.usar_gpu = false;
            let _ = crate::config::guardar(app, &config);
        }
    }
    BLOQUEADA.store(!config.usar_gpu, Ordering::Relaxed);
    let _ = MARCA.set(marca);
    if !bloqueada() {
        // Hasta que termine de cargar el primer modelo (ver `dictado`).
        marcar();
    }
}

/// Vulkan no se cargó en esta sesión (por la configuración o por un cierre anterior):
/// para usar la GPU hay que reiniciar Wister.
pub fn bloqueada() -> bool {
    BLOQUEADA.load(Ordering::Relaxed)
}

pub fn fallo_anterior() -> bool {
    FALLO.load(Ordering::Relaxed)
}

pub fn sin_vulkan() -> bool {
    SIN_VULKAN.load(Ordering::Relaxed)
}

/// Antes de algo con la GPU que podría cerrar la app (iniciar Vulkan, cargar un modelo).
pub fn marcar() {
    if let Some(m) = MARCA.get() {
        let _ = std::fs::write(m, b"");
    }
}

/// Después, si terminó (bien o con error, pero sin cerrar la app).
pub fn desmarcar() {
    if let Some(m) = MARCA.get() {
        let _ = std::fs::remove_file(m);
    }
}

#[cfg(all(windows, feature = "vulkan"))]
#[allow(non_upper_case_globals)]
mod carga_diferida {
    //! Hooks del cargador diferido de MSVC (`delayimp.h`). Solo se ocupan de `vulkan-1.dll`.
    use std::ffi::{c_char, c_void, CStr};
    use std::ptr::null;
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    #[repr(C)]
    struct DelayLoadProc {
        por_nombre: i32,
        /// Unión con el ordinal; Vulkan siempre se importa por nombre.
        nombre: *const c_char,
    }

    #[repr(C)]
    pub struct DelayLoadInfo {
        cb: u32,
        pidd: *const c_void,
        ppfn: *mut *const c_void,
        dll: *const c_char,
        dlp: DelayLoadProc,
        hmod_cur: *mut c_void,
        pfn_cur: *const c_void,
        ultimo_error: u32,
    }

    type Hook = unsafe extern "system" fn(u32, *const DelayLoadInfo) -> *const c_void;

    const ANTES_DE_CARGAR_DLL: u32 = 1;
    const NO_SE_CARGO_DLL: u32 = 3;
    const NO_SE_ENCONTRO_FUNCION: u32 = 4;
    const VK_ERROR_INITIALIZATION_FAILED: i32 = -3;

    #[no_mangle]
    #[used]
    pub static __pfnDliNotifyHook2: Hook = aviso;
    #[no_mangle]
    #[used]
    pub static __pfnDliFailureHook2: Hook = falla;

    unsafe fn es_vulkan(info: *const DelayLoadInfo) -> bool {
        !info.is_null()
            && !(*info).dll.is_null()
            && CStr::from_ptr((*info).dll)
                .to_bytes()
                .eq_ignore_ascii_case(b"vulkan-1.dll")
    }

    /// Un módulo que no tiene las funciones de Vulkan: buscarlas falla y se llega a
    /// `falla`, que devuelve las de mentira.
    unsafe fn modulo_sin_vulkan() -> *const c_void {
        GetModuleHandleW(null()) as *const c_void
    }

    unsafe extern "system" fn aviso(nota: u32, info: *const DelayLoadInfo) -> *const c_void {
        if nota == ANTES_DE_CARGAR_DLL && es_vulkan(info) && super::bloqueada() {
            return modulo_sin_vulkan();
        }
        null()
    }

    unsafe extern "system" fn falla(nota: u32, info: *const DelayLoadInfo) -> *const c_void {
        if !es_vulkan(info) {
            return null();
        }
        match nota {
            NO_SE_CARGO_DLL => {
                super::SIN_VULKAN.store(true, Ordering::Relaxed);
                modulo_sin_vulkan()
            }
            NO_SE_ENCONTRO_FUNCION => {
                let info = &*info;
                let nombre = (info.dlp.por_nombre != 0 && !info.dlp.nombre.is_null())
                    .then(|| CStr::from_ptr(info.dlp.nombre).to_bytes());
                if nombre == Some(b"vkGetInstanceProcAddr") {
                    vk_get_instance_proc_addr as *const c_void
                } else {
                    vk_falla as *const c_void
                }
            }
            _ => null(),
        }
    }

    /// whisper.cpp pide todas las funciones con esta: le da una que siempre falla. La
    /// primera que llama (`vkEnumerateInstanceVersion`) tira una excepción que whisper.cpp
    /// ataja, y Vulkan queda sin registrar.
    unsafe extern "system" fn vk_get_instance_proc_addr(
        _instancia: *const c_void,
        _nombre: *const c_char,
    ) -> *const c_void {
        vk_falla as *const c_void
    }

    /// Sirve para cualquier firma: en x64 los argumentos los limpia quien llama.
    unsafe extern "system" fn vk_falla() -> i32 {
        VK_ERROR_INITIALIZATION_FAILED
    }
}
