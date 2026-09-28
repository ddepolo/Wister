//! Volumen y silencio del micrófono, el mismo control que el de Configuración de sonido
//! de Windows (`IAudioEndpointVolume`): cambiarlo acá lo cambia para todas las apps.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Volumen {
    /// De 0 a 1.
    nivel: f32,
    silenciado: bool,
}

/// `nombre`: el micrófono de la configuración (`None`, el predeterminado).
#[tauri::command(async)]
pub fn obtener_volumen(nombre: Option<String>) -> Result<Volumen, String> {
    plataforma::volumen(nombre.as_deref(), None, None).map_err(|e| format!("{e:#}"))
}

/// Cambia el nivel, el silencio o los dos, y devuelve cómo quedó.
#[tauri::command(async)]
pub fn cambiar_volumen(
    nombre: Option<String>,
    nivel: Option<f32>,
    silenciado: Option<bool>,
) -> Result<Volumen, String> {
    plataforma::volumen(
        nombre.as_deref(),
        nivel.map(|n| n.clamp(0.0, 1.0)),
        silenciado,
    )
    .map_err(|e| format!("{e:#}"))
}

/// Posición en `nombres` del micrófono elegido: el nombre exacto o, si no está, el
/// primero que lo contiene sin distinguir mayúsculas (igual que al grabar).
#[cfg_attr(not(windows), allow(dead_code))]
fn elegir(nombres: &[String], buscado: &str) -> Option<usize> {
    nombres.iter().position(|n| n == buscado).or_else(|| {
        let buscado = buscado.to_lowercase();
        nombres
            .iter()
            .position(|n| n.to_lowercase().contains(&buscado))
    })
}

#[cfg(windows)]
mod plataforma {
    use anyhow::{Context, Result};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToStringAlloc,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED, STGM_READ,
    };

    use super::Volumen;

    pub fn volumen(
        nombre: Option<&str>,
        nivel: Option<f32>,
        silenciado: Option<bool>,
    ) -> Result<Volumen> {
        let nombre = nombre.map(str::to_owned);
        // En un hilo propio, con COM en modo multihilo: el de la ventana ya lo tiene
        // inicializado de otra forma.
        std::thread::spawn(move || unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .context("no se pudo inicializar COM")?;
            let r = aplicar(nombre.as_deref(), nivel, silenciado);
            CoUninitialize();
            r
        })
        .join()
        .map_err(|_| anyhow::anyhow!("falló el hilo del volumen"))?
    }

    unsafe fn aplicar(
        nombre: Option<&str>,
        nivel: Option<f32>,
        silenciado: Option<bool>,
    ) -> Result<Volumen> {
        let control: IAudioEndpointVolume = dispositivo(nombre)?
            .Activate(CLSCTX_ALL, None)
            .context("el micrófono no deja controlar su volumen")?;
        if let Some(n) = nivel {
            control
                .SetMasterVolumeLevelScalar(n, std::ptr::null())
                .context("no se pudo cambiar el volumen")?;
        }
        if let Some(s) = silenciado {
            control
                .SetMute(s, std::ptr::null())
                .context("no se pudo cambiar el silencio")?;
        }
        Ok(Volumen {
            nivel: control.GetMasterVolumeLevelScalar()?,
            silenciado: control.GetMute()?.as_bool(),
        })
    }

    /// El micrófono elegido o, si no está conectado, el predeterminado: el mismo que
    /// usaría el dictado.
    unsafe fn dispositivo(nombre: Option<&str>) -> Result<IMMDevice> {
        let enumerador: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("no se pudo consultar el audio de Windows")?;
        if let Some(buscado) = nombre {
            let coleccion = enumerador.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let mut dispositivos = Vec::new();
            let mut nombres = Vec::new();
            for i in 0..coleccion.GetCount()? {
                let d = coleccion.Item(i)?;
                nombres.push(nombre_de(&d).unwrap_or_default());
                dispositivos.push(d);
            }
            if let Some(i) = super::elegir(&nombres, buscado) {
                return Ok(dispositivos.swap_remove(i));
            }
        }
        enumerador
            .GetDefaultAudioEndpoint(eCapture, eConsole)
            .context("no hay un micrófono conectado")
    }

    /// El nombre que muestra Windows, que es el mismo que usa cpal.
    unsafe fn nombre_de(d: &IMMDevice) -> Result<String> {
        let propiedades = d.OpenPropertyStore(STGM_READ)?;
        let mut valor = propiedades.GetValue(&PKEY_Device_FriendlyName)?;
        let texto = PropVariantToStringAlloc(&valor);
        let _ = PropVariantClear(&mut valor);
        let texto = texto?;
        let s = texto.to_string();
        CoTaskMemFree(Some(texto.0 as *const _));
        Ok(s?)
    }
}

#[cfg(not(windows))]
mod plataforma {
    use super::Volumen;

    pub fn volumen(
        _nombre: Option<&str>,
        _nivel: Option<f32>,
        _silenciado: Option<bool>,
    ) -> anyhow::Result<Volumen> {
        anyhow::bail!("el volumen del micrófono todavía solo se puede cambiar en Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_elige_el_nombre_exacto_antes_que_uno_parecido() {
        let nombres = vec![
            "Micrófono (USB PnP Audio Device) 2".to_string(),
            "Micrófono (USB PnP Audio Device)".to_string(),
        ];
        assert_eq!(
            elegir(&nombres, "Micrófono (USB PnP Audio Device)"),
            Some(1)
        );
        assert_eq!(elegir(&nombres, "usb pnp"), Some(0));
        assert_eq!(elegir(&nombres, "Blue Yeti"), None);
    }
}
