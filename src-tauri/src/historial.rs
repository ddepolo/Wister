//! Historial de dictados en SQLite (`historial.db` en la carpeta de configuración de
//! la app) y los comandos que lo leen y lo borran desde la UI.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use rusqlite::functions::FunctionFlags;
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Cambios del esquema, en orden: `PRAGMA user_version` dice cuántos ya se aplicaron.
const MIGRACIONES: &[&str] = &["CREATE TABLE dictados (
        id INTEGER PRIMARY KEY,
        fecha INTEGER NOT NULL, -- milisegundos desde 1970, UTC
        texto TEXT NOT NULL,
        audio_ms INTEGER NOT NULL,
        palabras INTEGER NOT NULL,
        app TEXT -- título de la ventana donde se pegó
    );
    CREATE INDEX dictados_fecha ON dictados (fecha);"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dictado {
    pub id: i64,
    pub fecha: i64,
    pub texto: String,
    pub audio_ms: u32,
    pub palabras: u32,
    pub app: Option<String>,
}

/// Evento `historial`, para que la UI se actualice sin volver a pedir todo.
#[derive(Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
enum Cambio {
    Agregado { dictado: Dictado },
    Borrado { id: i64 },
    BorradoTodo,
    Error { mensaje: String },
}

fn abrir(ruta: &Path) -> Result<Connection> {
    if let Some(dir) = ruta.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("no se pudo crear {}", dir.display()))?;
    }
    let conexion = Connection::open(ruta)
        .with_context(|| format!("no se pudo abrir el historial ({})", ruta.display()))?;
    preparar(conexion)
}

fn preparar(conexion: Connection) -> Result<Connection> {
    // `secure_delete`: sin esto, lo borrado sigue en el archivo hasta que SQLite reuse
    // esas páginas. WAL con `synchronous = NORMAL` no espera al disco en cada dictado
    // (un corte de luz puede perder el último, pero no corrompe la base).
    conexion.execute_batch(
        "PRAGMA secure_delete = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;",
    )?;
    conexion.create_scalar_function(
        "plegar",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|t| plegar(&t))),
    )?;
    let version: u32 = conexion.query_row("PRAGMA user_version", [], |f| f.get(0))?;
    for (i, migracion) in MIGRACIONES.iter().enumerate().skip(version as usize) {
        conexion
            .execute_batch(&format!(
                "BEGIN; {migracion}; PRAGMA user_version = {}; COMMIT;",
                i + 1
            ))
            .context("no se pudo actualizar el formato del historial")?;
    }
    Ok(conexion)
}

/// Minúsculas y sin tildes, para que "cancion" encuentre "Canción". La `ñ` se deja:
/// en español es otra letra.
fn plegar(texto: &str) -> String {
    texto
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Palabras para las estadísticas: lo que queda entre espacios y tiene alguna letra o
/// número (un guion suelto no cuenta).
pub fn contar_palabras(texto: &str) -> u32 {
    texto
        .split_whitespace()
        .filter(|p| p.chars().any(char::is_alphanumeric))
        .count() as u32
}

fn agregar(
    conexion: &Connection,
    fecha: i64,
    texto: &str,
    audio_ms: u32,
    app: Option<&str>,
) -> Result<Dictado> {
    let palabras = contar_palabras(texto);
    conexion.execute(
        "INSERT INTO dictados (fecha, texto, audio_ms, palabras, app) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![fecha, texto, audio_ms, palabras, app],
    )?;
    Ok(Dictado {
        id: conexion.last_insert_rowid(),
        fecha,
        texto: texto.to_owned(),
        audio_ms,
        palabras,
        app: app.map(str::to_owned),
    })
}

/// Los dictados más nuevos primero; `antes_de` (un id) pide la página siguiente.
fn listar(
    conexion: &Connection,
    busqueda: &str,
    antes_de: Option<i64>,
    limite: u32,
) -> Result<Vec<Dictado>> {
    let busqueda = plegar(busqueda.trim());
    let mut consulta = conexion.prepare_cached(
        "SELECT id, fecha, texto, audio_ms, palabras, app FROM dictados
         WHERE (?1 = '' OR instr(plegar(texto), ?1) > 0 OR instr(plegar(app), ?1) > 0)
           AND (?2 IS NULL OR id < ?2)
         ORDER BY id DESC LIMIT ?3",
    )?;
    let filas = consulta.query_map(params![busqueda, antes_de, limite], |f| {
        Ok(Dictado {
            id: f.get(0)?,
            fecha: f.get(1)?,
            texto: f.get(2)?,
            audio_ms: f.get(3)?,
            palabras: f.get(4)?,
            app: f.get(5)?,
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

fn borrar(conexion: &Connection, id: i64) -> Result<bool> {
    Ok(conexion.execute("DELETE FROM dictados WHERE id = ?1", [id])? > 0)
}

fn borrar_todo(conexion: &Connection) -> Result<()> {
    conexion.execute("DELETE FROM dictados", [])?;
    // Devuelve el espacio al disco: el archivo no queda del tamaño que tenía.
    conexion.execute_batch("VACUUM")?;
    Ok(())
}

/// Velocidad de tipeo con la que se compara para calcular el tiempo ahorrado.
const PALABRAS_POR_MINUTO_TIPEANDO: u64 = 40;

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Estadisticas {
    pub palabras_hoy: u64,
    /// Hoy y los seis días anteriores.
    pub palabras_semana: u64,
    pub palabras_total: u64,
    pub dictados_total: u64,
    /// Lo que se hubiera tardado tipeando menos lo que se tardó hablando.
    pub segundos_ahorrados: u64,
    /// Días seguidos con algún dictado, hasta hoy o hasta ayer (la racha no se corta
    /// por no haber dictado todavía hoy).
    pub racha_dias: u32,
    /// Por minuto de audio grabado, con las pausas incluidas.
    pub palabras_por_minuto: u32,
}

#[derive(Debug, Clone, Copy)]
struct Dia {
    /// Cuántos días antes de hoy (0 = hoy), según el calendario local.
    atras: i64,
    palabras: u64,
    audio_ms: u64,
    dictados: u64,
}

fn por_dia(conexion: &Connection, ahora: i64) -> Result<Vec<Dia>> {
    let mut consulta = conexion.prepare_cached(
        "SELECT CAST(julianday(date(?1 / 1000, 'unixepoch', 'localtime'))
                   - julianday(date(fecha / 1000, 'unixepoch', 'localtime')) AS INTEGER) AS atras,
                SUM(palabras), SUM(audio_ms), COUNT(*)
         FROM dictados GROUP BY atras ORDER BY atras",
    )?;
    let filas = consulta.query_map([ahora], |f| {
        Ok(Dia {
            atras: f.get(0)?,
            palabras: f.get::<_, i64>(1)?.max(0) as u64,
            audio_ms: f.get::<_, i64>(2)?.max(0) as u64,
            dictados: f.get::<_, i64>(3)?.max(0) as u64,
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// `dias` ordenados por `atras`, de hoy hacia atrás.
fn resumir(dias: &[Dia]) -> Estadisticas {
    let palabras_total: u64 = dias.iter().map(|d| d.palabras).sum();
    let audio_ms: u64 = dias.iter().map(|d| d.audio_ms).sum();
    let segundos_tipeando = palabras_total * 60 / PALABRAS_POR_MINUTO_TIPEANDO;

    // Un día "en el futuro" (se atrasó el reloj) cuenta en los totales pero no en la racha.
    let mut racha = 0;
    let mut atras = dias.iter().map(|d| d.atras).filter(|&a| a >= 0).peekable();
    if let Some(&primero) = atras.peek() {
        if primero <= 1 {
            racha = atras
                .zip(primero..)
                .take_while(|(a, esperado)| a == esperado)
                .count() as u32;
        }
    }

    Estadisticas {
        palabras_hoy: dias
            .iter()
            .filter(|d| d.atras == 0)
            .map(|d| d.palabras)
            .sum(),
        palabras_semana: dias
            .iter()
            .filter(|d| (0..7).contains(&d.atras))
            .map(|d| d.palabras)
            .sum(),
        palabras_total,
        dictados_total: dias.iter().map(|d| d.dictados).sum(),
        segundos_ahorrados: segundos_tipeando.saturating_sub(audio_ms / 1000),
        racha_dias: racha,
        palabras_por_minuto: (palabras_total * 60_000).checked_div(audio_ms).unwrap_or(0) as u32,
    }
}

fn ahora() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Conexión al historial, que se abre la primera vez que se usa.
///
/// Se registra antes del `setup` (no necesita nada de él), así que los comandos pueden
/// usar `State` sin problema.
#[derive(Default)]
pub struct Historial(Mutex<Option<Connection>>);

impl Historial {
    fn con<T>(&self, app: &AppHandle, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let mut conexion = self
            .0
            .lock()
            .map_err(|_| anyhow!("el historial quedó bloqueado por un error anterior"))?;
        let conexion = match conexion.as_mut() {
            Some(c) => c,
            None => {
                let ruta = app
                    .path()
                    .app_config_dir()
                    .context("no se encontró la carpeta de configuración")?
                    .join("historial.db");
                conexion.insert(abrir(&ruta)?)
            }
        };
        f(conexion)
    }
}

/// Guarda un dictado y le avisa a la UI. Lo llama el hilo de dictado.
pub fn anotar(app: &AppHandle, texto: &str, audio_ms: u64, destino: Option<&str>) {
    let Some(historial) = app.try_state::<Historial>() else {
        return;
    };
    let fecha = ahora();
    let audio_ms = u32::try_from(audio_ms).unwrap_or(u32::MAX);
    let cambio = match historial.con(app, |c| agregar(c, fecha, texto, audio_ms, destino)) {
        Ok(dictado) => Cambio::Agregado { dictado },
        Err(e) => Cambio::Error {
            mensaje: format!("No se pudo guardar el dictado en el historial: {e:#}"),
        },
    };
    let _ = app.emit("historial", cambio);
}

#[tauri::command]
pub fn listar_historial(
    app: AppHandle,
    historial: State<Historial>,
    busqueda: String,
    antes_de: Option<i64>,
    limite: u32,
) -> Result<Vec<Dictado>, String> {
    historial
        .con(&app, |c| listar(c, &busqueda, antes_de, limite))
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn estadisticas(app: AppHandle, historial: State<Historial>) -> Result<Estadisticas, String> {
    historial
        .con(&app, |c| Ok(resumir(&por_dia(c, ahora())?)))
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn borrar_dictado(app: AppHandle, historial: State<Historial>, id: i64) -> Result<(), String> {
    if historial
        .con(&app, |c| borrar(c, id))
        .map_err(|e| format!("{e:#}"))?
    {
        let _ = app.emit("historial", Cambio::Borrado { id });
    }
    Ok(())
}

#[tauri::command]
pub fn borrar_historial(app: AppHandle, historial: State<Historial>) -> Result<(), String> {
    historial
        .con(&app, borrar_todo)
        .map_err(|e| format!("{e:#}"))?;
    let _ = app.emit("historial", Cambio::BorradoTodo);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en_memoria() -> Connection {
        preparar(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn con_dictados(textos: &[(&str, Option<&str>)]) -> Connection {
        let c = en_memoria();
        for (i, (texto, app)) in textos.iter().enumerate() {
            agregar(&c, i as i64 * 1000, texto, 2000, *app).unwrap();
        }
        c
    }

    fn textos(dictados: &[Dictado]) -> Vec<&str> {
        dictados.iter().map(|d| d.texto.as_str()).collect()
    }

    #[test]
    fn se_listan_del_mas_nuevo_al_mas_viejo() {
        let c = con_dictados(&[("uno", None), ("dos", None), ("tres", None)]);
        assert_eq!(
            textos(&listar(&c, "", None, 10).unwrap()),
            ["tres", "dos", "uno"]
        );
    }

    #[test]
    fn la_pagina_siguiente_arranca_despues_del_ultimo_id() {
        let c = con_dictados(&[("uno", None), ("dos", None), ("tres", None)]);
        let primera = listar(&c, "", None, 2).unwrap();
        assert_eq!(textos(&primera), ["tres", "dos"]);
        let segunda = listar(&c, "", Some(primera[1].id), 2).unwrap();
        assert_eq!(textos(&segunda), ["uno"]);
    }

    #[test]
    fn la_busqueda_ignora_mayusculas_y_tildes() {
        let c = con_dictados(&[("Una CANCIÓN nueva", None), ("otra cosa", None)]);
        assert_eq!(
            textos(&listar(&c, "cancion", None, 10).unwrap()),
            ["Una CANCIÓN nueva"]
        );
        assert_eq!(
            textos(&listar(&c, "  Canción ", None, 10).unwrap()),
            ["Una CANCIÓN nueva"]
        );
    }

    #[test]
    fn la_busqueda_encuentra_por_la_app_destino() {
        let c = con_dictados(&[("hola", Some("General | Slack")), ("chau", None)]);
        assert_eq!(textos(&listar(&c, "slack", None, 10).unwrap()), ["hola"]);
    }

    #[test]
    fn la_busqueda_no_trata_comodines_de_sql() {
        let c = con_dictados(&[("cien por ciento", None), ("100% seguro", None)]);
        assert_eq!(textos(&listar(&c, "%", None, 10).unwrap()), ["100% seguro"]);
    }

    #[test]
    fn se_guardan_las_palabras_y_la_app() {
        let c = en_memoria();
        let d = agregar(&c, 5, "Hola, ¿qué tal? - bien", 1500, Some("Bloc de notas")).unwrap();
        assert_eq!(d.palabras, 4);
        assert_eq!(listar(&c, "", None, 1).unwrap(), [d]);
    }

    #[test]
    fn borrar_un_dictado_deja_los_demas() {
        let c = con_dictados(&[("uno", None), ("dos", None)]);
        let dos = listar(&c, "dos", None, 1).unwrap()[0].id;
        assert!(borrar(&c, dos).unwrap());
        assert!(!borrar(&c, dos).unwrap());
        assert_eq!(textos(&listar(&c, "", None, 10).unwrap()), ["uno"]);
    }

    #[test]
    fn borrar_todo_vacia_el_historial() {
        let c = con_dictados(&[("uno", None), ("dos", None)]);
        borrar_todo(&c).unwrap();
        assert!(listar(&c, "", None, 10).unwrap().is_empty());
    }

    #[test]
    fn abrir_dos_veces_no_repite_las_migraciones() {
        let dir = std::env::temp_dir().join(format!("wister-historial-{}", std::process::id()));
        let ruta = dir.join("historial.db");
        agregar(&abrir(&ruta).unwrap(), 1, "hola", 1000, None).unwrap();
        let c = abrir(&ruta).unwrap();
        assert_eq!(textos(&listar(&c, "", None, 10).unwrap()), ["hola"]);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn dia(atras: i64, palabras: u64) -> Dia {
        Dia {
            atras,
            palabras,
            audio_ms: palabras * 400,
            dictados: 1,
        }
    }

    #[test]
    fn sin_dictados_las_estadisticas_dan_cero() {
        assert_eq!(resumir(&[]), Estadisticas::default());
    }

    #[test]
    fn las_palabras_se_suman_por_hoy_semana_y_total() {
        let e = resumir(&[dia(0, 10), dia(3, 20), dia(6, 30), dia(7, 40)]);
        assert_eq!(e.palabras_hoy, 10);
        assert_eq!(e.palabras_semana, 60);
        assert_eq!(e.palabras_total, 100);
        assert_eq!(e.dictados_total, 4);
    }

    #[test]
    fn el_tiempo_ahorrado_descuenta_lo_que_se_hablo() {
        // 400 palabras: 10 min tipeando, 160 s hablando (400 ms por palabra).
        let e = resumir(&[dia(0, 400)]);
        assert_eq!(e.segundos_ahorrados, 600 - 160);
        assert_eq!(e.palabras_por_minuto, 150);
    }

    #[test]
    fn la_racha_cuenta_dias_seguidos_hasta_hoy() {
        assert_eq!(
            resumir(&[dia(0, 1), dia(1, 1), dia(2, 1), dia(4, 1)]).racha_dias,
            3
        );
    }

    #[test]
    fn la_racha_sigue_si_todavia_no_se_dicto_hoy() {
        assert_eq!(resumir(&[dia(1, 1), dia(2, 1)]).racha_dias, 2);
    }

    #[test]
    fn la_racha_se_corta_si_ayer_no_se_dicto() {
        assert_eq!(resumir(&[dia(2, 1), dia(3, 1)]).racha_dias, 0);
    }

    #[test]
    fn un_dia_en_el_futuro_no_rompe_la_racha() {
        let e = resumir(&[dia(-1, 5), dia(0, 1), dia(1, 1)]);
        assert_eq!(e.racha_dias, 2);
        assert_eq!(e.palabras_total, 7);
    }

    #[test]
    fn por_dia_agrupa_segun_el_calendario() {
        const DIA: i64 = 86_400_000;
        let ahora = 1_790_510_400_000; // un mediodía UTC
        let c = en_memoria();
        agregar(&c, ahora, "uno dos", 1000, None).unwrap();
        agregar(&c, ahora - 60_000, "tres", 1000, None).unwrap();
        agregar(&c, ahora - 2 * DIA, "cuatro cinco seis", 1000, None).unwrap();
        let dias = por_dia(&c, ahora).unwrap();
        let resumen: Vec<_> = dias
            .iter()
            .map(|d| (d.atras, d.palabras, d.dictados))
            .collect();
        assert_eq!(resumen, [(0, 3, 2), (2, 3, 1)]);
    }

    #[test]
    fn contar_palabras_ignora_signos_sueltos() {
        assert_eq!(contar_palabras(""), 0);
        assert_eq!(contar_palabras("  hola   mundo "), 2);
        assert_eq!(contar_palabras("sí — no ... 3 gatos"), 4);
    }
}
