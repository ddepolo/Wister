//! Datos de la PC para el diagnóstico: versión de Windows, CPU, memoria y placas de video.

use wister_core::stt;

#[derive(Debug, Clone)]
pub struct Sistema {
    pub windows: String,
    pub cpu: String,
    pub nucleos: usize,
    pub ram_mb: u64,
    /// Placas de video instaladas, con su driver (según Windows, no según Whisper).
    pub placas: Vec<String>,
}

impl Sistema {
    pub fn detectar() -> Self {
        Self {
            windows: plataforma::windows(),
            cpu: plataforma::cpu(),
            nucleos: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(0),
            ram_mb: plataforma::ram_mb(),
            placas: plataforma::placas(),
        }
    }

    /// Una línea para el registro al arrancar.
    pub fn resumen(&self) -> String {
        format!(
            "{} · {} ({} hilos) · {} MB de RAM · {}",
            self.windows,
            self.cpu,
            self.nucleos,
            self.ram_mb,
            descripcion_gpu()
        )
    }
}

/// RAM instalada en MB; 0 si no se sabe. `WISTER_RAM_MB` la simula, para probar los
/// avisos de memoria en una PC con mucha RAM.
pub fn ram_mb() -> u64 {
    std::env::var("WISTER_RAM_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(plataforma::ram_mb)
}

/// Variante del binario y la GPU que usa Whisper.
pub fn descripcion_gpu() -> String {
    match stt::gpu_backend() {
        None => "compilado solo para CPU".into(),
        Some(backend) => match stt::gpu_devices() {
            [] if backend == "Vulkan" => "Vulkan sin GPU compatible: usa la CPU".into(),
            [] => backend.into(),
            gpus => {
                let nombres: Vec<String> = gpus
                    .iter()
                    .map(|g| format!("{} ({} MB)", g.name, g.vram_mb))
                    .collect();
                format!("{backend}: {}", nombres.join(", "))
            }
        },
    }
}

/// `ProductName` sigue diciendo "Windows 10" en Windows 11: lo que las distingue es el build.
#[cfg_attr(not(windows), allow(dead_code))]
fn nombre_windows(producto: &str, build: u32) -> String {
    if build >= 22000 {
        producto.replacen("Windows 10", "Windows 11", 1)
    } else {
        producto.to_owned()
    }
}

#[cfg(windows)]
mod plataforma {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_QWORD, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    const VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    const CPU: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
    /// Clase de dispositivo "Adaptadores de pantalla"; cada placa es una subclave 0000, 0001...
    const PLACAS: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

    pub fn windows() -> String {
        let producto = texto(VERSION, "ProductName").unwrap_or_else(|| "Windows".into());
        let build: u32 = texto(VERSION, "CurrentBuildNumber")
            .and_then(|b| b.parse().ok())
            .unwrap_or(0);
        let mut s = super::nombre_windows(&producto, build);
        if let Some(v) = texto(VERSION, "DisplayVersion") {
            s += &format!(" {v}");
        }
        s += &format!(" (build {build}");
        if let Some(ubr) = numero(VERSION, "UBR") {
            s += &format!(".{ubr}");
        }
        s + ")"
    }

    pub fn cpu() -> String {
        texto(CPU, "ProcessorNameString").unwrap_or_else(|| "CPU desconocida".into())
    }

    pub fn ram_mb() -> u64 {
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
            return 0;
        }
        m.ullTotalPhys / (1024 * 1024)
    }

    pub fn placas() -> Vec<String> {
        // No siempre son consecutivas (quedan huecos al desinstalar drivers).
        (0..16)
            .filter_map(|i| {
                let clave = format!(r"{PLACAS}\{i:04}");
                let nombre = texto(&clave, "DriverDesc")?;
                let mut s = nombre;
                if let Some(v) = texto(&clave, "DriverVersion") {
                    s += &format!(", driver {v}");
                }
                if let Some(bytes) = numero(&clave, "HardwareInformation.qwMemorySize") {
                    s += &format!(", {} MB", bytes / (1024 * 1024));
                }
                Some(s)
            })
            .collect()
    }

    fn texto(clave: &str, valor: &str) -> Option<String> {
        let (clave, valor) = (wide(clave), wide(valor));
        let leer = |datos: *mut c_void, bytes: &mut u32| unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                clave.as_ptr(),
                valor.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                datos,
                bytes,
            )
        };
        let mut bytes = 0u32;
        if leer(null_mut(), &mut bytes) != ERROR_SUCCESS || bytes == 0 {
            return None;
        }
        let mut buffer = vec![0u16; (bytes as usize).div_ceil(2)];
        if leer(buffer.as_mut_ptr().cast(), &mut bytes) != ERROR_SUCCESS {
            return None;
        }
        let largo = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let s = String::from_utf16_lossy(&buffer[..largo]).trim().to_owned();
        (!s.is_empty()).then_some(s)
    }

    fn numero(clave: &str, valor: &str) -> Option<u64> {
        let (clave, valor) = (wide(clave), wide(valor));
        // Un DWORD ocupa los 4 bytes bajos; los altos quedan en cero.
        let mut n = 0u64;
        let mut bytes = size_of::<u64>() as u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                clave.as_ptr(),
                valor.as_ptr(),
                RRF_RT_REG_DWORD | RRF_RT_REG_QWORD,
                null_mut(),
                (&mut n as *mut u64).cast(),
                &mut bytes,
            )
        };
        (r == ERROR_SUCCESS).then_some(n)
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(not(windows))]
mod plataforma {
    pub fn windows() -> String {
        std::env::consts::OS.into()
    }

    pub fn cpu() -> String {
        "CPU desconocida".into()
    }

    pub fn ram_mb() -> u64 {
        0
    }

    pub fn placas() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_se_reconoce_por_el_build() {
        assert_eq!(nombre_windows("Windows 10 Pro", 26100), "Windows 11 Pro");
        assert_eq!(nombre_windows("Windows 10 Pro", 19045), "Windows 10 Pro");
    }
}
