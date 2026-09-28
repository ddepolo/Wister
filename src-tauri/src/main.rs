// Sin consola en release: es una app de bandeja.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actualizar;
mod config;
mod dictado;
mod historial;
mod hotkey;
mod overlay;
mod pegar;
mod sonidos;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

const VENTANA_CONFIG: &str = "config";

fn main() {
    tauri::Builder::default()
        // Una segunda instancia instalaría otro hook y cada dictado se pegaría dos veces:
        // en su lugar, se muestra la ventana de la que ya está corriendo.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            mostrar_config(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(dictado::EstadoActual::default())
        .manage(config::Descargas::default())
        .manage(historial::Historial::default())
        .invoke_handler(tauri::generate_handler![
            dictado::estado_actual,
            config::obtener_config,
            config::guardar_config,
            config::listar_modelos,
            config::listar_microfonos,
            config::descargar_modelo,
            config::autoarranque,
            config::cambiar_autoarranque,
            config::probar_microfono,
            config::detener_prueba_microfono,
            config::pausar_atajo,
            historial::listar_historial,
            historial::borrar_dictado,
            historial::borrar_historial,
            historial::estadisticas,
            salir,
            actualizar::buscar_actualizacion,
            actualizar::instalar_actualizacion,
        ])
        .setup(|app| {
            crear_bandeja(app.handle())?;
            overlay::preparar(app.handle())?;
            iniciar_dictado(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Cerrar la ventana de configuración la oculta: la app sigue en la bandeja.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar Wister");
}

/// Cierra Wister del todo (cerrar la ventana solo la esconde en la bandeja).
#[tauri::command]
fn salir(app: AppHandle) {
    app.exit(0);
}

fn crear_bandeja(app: &AppHandle) -> tauri::Result<()> {
    let config = MenuItem::with_id(app, "config", "Abrir Wister", true, None::<&str>)?;
    let salir = MenuItem::with_id(app, "salir", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&config, &salir])?;

    let mut bandeja = TrayIconBuilder::with_id("wister")
        .tooltip("Wister")
        .menu(&menu)
        // El menú queda en el clic derecho: con el izquierdo, el primer clic abriría el
        // menú y el doble clic nunca llegaría.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|bandeja, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                mostrar_config(bandeja.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "config" => mostrar_config(app),
            "salir" => app.exit(0),
            _ => {}
        });
    if let Some(icono) = app.default_window_icon() {
        bandeja = bandeja.icon(icono.clone());
    }
    bandeja.build(app)?;
    Ok(())
}

fn iniciar_dictado(app: &AppHandle) -> anyhow::Result<()> {
    let config = config::cargar(app);
    app.manage(config::ConfigActual(std::sync::Mutex::new(config.clone())));
    if !config.asistente_completo {
        mostrar_config(app);
    }

    let (tx, rx) = std::sync::mpsc::channel();
    app.manage(dictado::CanalDictado(tx.clone()));
    #[cfg(windows)]
    hotkey::escuchar(&config.atajo, move |evento| {
        let _ = tx.send(dictado::Mensaje::Atajo(evento));
    })?;
    #[cfg(not(windows))]
    drop(tx);
    dictado::iniciar(app.clone(), rx, config)
}

fn mostrar_config(app: &AppHandle) {
    if let Some(ventana) = app.get_webview_window(VENTANA_CONFIG) {
        let _ = ventana.show();
        let _ = ventana.unminimize();
        let _ = ventana.set_focus();
    }
}
