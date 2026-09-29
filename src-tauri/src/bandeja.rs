//! Ícono de la bandeja: doble clic abre la ventana; el clic derecho, el menú, que
//! también deja elegir el micrófono.

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use wister_core::audio;

use crate::config;
use crate::dictado::{CanalDictado, Mensaje, UltimoDictado};

const ID: &str = "wister";
const VENTANA_CONFIG: &str = "config";
const MIC_PREDETERMINADO: &str = "mic-predeterminado";
const PREFIJO_MIC: &str = "mic:";
const COPIAR_ULTIMO: &str = "copiar-ultimo";

pub fn crear(app: &AppHandle) -> tauri::Result<()> {
    let mut bandeja = TrayIconBuilder::with_id(ID)
        .tooltip("Wister")
        .menu(&menu(app)?)
        // El menú queda en el clic derecho: con el izquierdo, el primer clic abriría el
        // menú y el doble clic nunca llegaría.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|bandeja, event| match event {
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => mostrar_config(bandeja.app_handle()),
            // Al pasar el mouse se relee la lista, antes de que se pueda abrir el menú:
            // puede que se haya enchufado o desenchufado un micrófono.
            TrayIconEvent::Enter { .. } => actualizar(bandeja.app_handle()),
            _ => {}
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "config" => mostrar_config(app),
            "salir" => app.exit(0),
            COPIAR_ULTIMO => {
                if let Some(canal) = app.try_state::<CanalDictado>() {
                    let _ = canal.0.send(Mensaje::CopiarUltimo);
                }
            }
            MIC_PREDETERMINADO => elegir_microfono(app, None),
            id => {
                if let Some(nombre) = id.strip_prefix(PREFIJO_MIC) {
                    elegir_microfono(app, Some(nombre.to_owned()));
                }
            }
        });
    if let Some(icono) = app.default_window_icon() {
        bandeja = bandeja.icon(icono.clone());
    }
    bandeja.build(app)?;
    Ok(())
}

/// Rearma el menú con los micrófonos conectados y el elegido en la configuración.
pub fn actualizar(app: &AppHandle) {
    let Some(bandeja) = app.tray_by_id(ID) else {
        return;
    };
    match menu(app) {
        Ok(m) => {
            let _ = bandeja.set_menu(Some(m));
        }
        Err(e) => log::warn!("no se pudo armar el menú de la bandeja: {e}"),
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let abrir = MenuItem::with_id(app, "config", "Abrir Wister", true, None::<&str>)?;
    let hay_ultimo = app
        .try_state::<UltimoDictado>()
        .is_some_and(|u| u.0.lock().is_ok_and(|t| t.is_some()));
    let copiar = MenuItem::with_id(
        app,
        COPIAR_ULTIMO,
        "Copiar el último dictado",
        hay_ultimo,
        None::<&str>,
    )?;
    let microfono = submenu_microfono(app)?;
    let separador = PredefinedMenuItem::separator(app)?;
    let salir = MenuItem::with_id(app, "salir", "Salir", true, None::<&str>)?;
    Menu::with_items(app, &[&abrir, &copiar, &microfono, &separador, &salir])
}

fn submenu_microfono(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let elegido = config::obtener_config(app.clone()).microfono;
    let conectados = audio::list_input_devices().unwrap_or_else(|e| {
        log::warn!("no se pudieron listar los micrófonos: {e:#}");
        Vec::new()
    });
    let predeterminado = match conectados.iter().find(|m| m.is_default) {
        Some(m) => format!("Predeterminado de Windows ({})", texto(&m.name)),
        None => "Predeterminado de Windows".into(),
    };

    let mut items = vec![CheckMenuItem::with_id(
        app,
        MIC_PREDETERMINADO,
        predeterminado,
        true,
        elegido.is_none(),
        None::<&str>,
    )?];
    for m in &conectados {
        items.push(CheckMenuItem::with_id(
            app,
            format!("{PREFIJO_MIC}{}", m.name),
            texto(&m.name),
            true,
            elegido.as_deref() == Some(m.name.as_str()),
            None::<&str>,
        )?);
    }
    // Si el elegido no está conectado, se muestra igual: el dictado usa el
    // predeterminado hasta que vuelva.
    if let Some(nombre) = elegido.filter(|e| !conectados.iter().any(|m| &m.name == e)) {
        items.push(CheckMenuItem::with_id(
            app,
            format!("{PREFIJO_MIC}{nombre}"),
            format!("{} (no conectado)", texto(&nombre)),
            false,
            true,
            None::<&str>,
        )?);
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    Submenu::with_items(app, "Micrófono", true, &refs)
}

/// En los menús de Windows `&` marca la tecla de acceso: se duplica para que se vea.
fn texto(nombre: &str) -> String {
    nombre.replace('&', "&&")
}

fn elegir_microfono(app: &AppHandle, nombre: Option<String>) {
    let mut c = config::obtener_config(app.clone());
    c.microfono = nombre;
    if let Err(e) = config::guardar_config(app.clone(), c) {
        log::warn!("no se pudo cambiar el micrófono: {e}");
    }
}

pub fn mostrar_config(app: &AppHandle) {
    if let Some(ventana) = app.get_webview_window(VENTANA_CONFIG) {
        let _ = ventana.show();
        let _ = ventana.unminimize();
        let _ = ventana.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_ampersand_se_ve_en_el_menu() {
        assert_eq!(texto("Micrófono (A&B)"), "Micrófono (A&&B)");
    }
}
