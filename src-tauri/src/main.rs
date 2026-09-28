// Sin consola en release: es una app de bandeja.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actualizar;
mod bandeja;
mod config;
mod diagnostico;
mod dictado;
mod historial;
mod hotkey;
mod overlay;
mod pegar;
mod registro;
mod rendimiento;
mod sistema;
mod sonidos;
mod volumen;

use tauri::{AppHandle, Manager, WindowEvent};

fn main() {
    tauri::Builder::default()
        // Una segunda instancia instalaría otro hook y cada dictado se pegaría dos veces:
        // en su lugar, se muestra la ventana de la que ya está corriendo.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            bandeja::mostrar_config(app)
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
            diagnostico::exportar_diagnostico,
            rendimiento::iniciar_prueba_rendimiento,
            rendimiento::medir_prueba_rendimiento,
            rendimiento::cancelar_prueba_rendimiento,
            volumen::obtener_volumen,
            volumen::cambiar_volumen,
        ])
        .setup(|app| {
            if let Some(carpeta) = registro::carpeta(app.handle()) {
                registro::iniciar(&carpeta);
            }
            log::info!(
                "Wister {} iniciado ({})",
                app.package_info().version,
                wister_core::stt::gpu_backend().unwrap_or("solo CPU")
            );
            bandeja::crear(app.handle())?;
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

fn iniciar_dictado(app: &AppHandle) -> anyhow::Result<()> {
    let config = config::cargar(app);
    app.manage(config::ConfigActual(std::sync::Mutex::new(config.clone())));
    if !config.asistente_completo {
        bandeja::mostrar_config(app);
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
