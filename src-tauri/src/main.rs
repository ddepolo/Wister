// Sin consola en release: es una app de bandeja.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actualizar;
mod bandeja;
mod config;
mod diagnostico;
mod diccionario;
mod dictado;
mod gpu;
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
        // Después del de instancia única y antes que todo lo que pueda tocar Vulkan.
        .plugin(gpu::plugin())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(dictado::EstadoActual::default())
        .manage(dictado::UltimoDictado::default())
        .manage(config::Descargas::default())
        .manage(historial::Historial::default())
        .manage(actualizar::Pendiente::default())
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
            reiniciar,
            abrir_pagina,
            diccionario::comandos_de_voz,
            actualizar::buscar_actualizacion,
            actualizar::instalar_actualizacion,
            actualizar::actualizacion_pendiente,
            diagnostico::exportar_diagnostico,
            rendimiento::iniciar_prueba_rendimiento,
            rendimiento::medir_prueba_rendimiento,
            rendimiento::cancelar_prueba_rendimiento,
            volumen::obtener_volumen,
            volumen::cambiar_volumen,
            sistema::estado_gpu,
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
            actualizar::buscar_periodicamente(app.handle().clone());
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

/// Páginas del proyecto que puede abrir la ventana, en el navegador. Son fijas: la
/// interfaz no puede pedir que se abra cualquier dirección.
#[tauri::command]
fn abrir_pagina(pagina: &str) -> Result<(), String> {
    let url = match pagina {
        "repositorio" => "https://github.com/ddepolo/Wister",
        "problemas" => "https://github.com/ddepolo/Wister/issues",
        "licencia" => "https://github.com/ddepolo/Wister/blob/main/LICENSE",
        otra => return Err(format!("página desconocida: {otra}")),
    };
    #[cfg(windows)]
    {
        // El Explorador abre las direcciones web con el navegador predeterminado.
        std::process::Command::new("explorer")
            .arg(url)
            .spawn()
            .map_err(|e| format!("no se pudo abrir el navegador: {e}"))?;
    }
    #[cfg(not(windows))]
    let _ = url;
    Ok(())
}

/// Cierra Wister del todo (cerrar la ventana solo la esconde en la bandeja).
#[tauri::command]
fn salir(app: AppHandle) {
    app.exit(0);
}

/// Para empezar a usar la placa de video si Vulkan no se cargó al arrancar.
#[tauri::command]
fn reiniciar(app: AppHandle) {
    app.restart();
}

fn iniciar_dictado(app: &AppHandle) -> anyhow::Result<()> {
    let config = config::cargar(app);
    config::aplicar_tema(app, config.tema);
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
