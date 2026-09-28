//! Diccionario personal:
//!
//! - **Vocabulario**: palabras que Whisper no conoce (nombres propios, marcas, jerga)
//!   y que se le pasan como pista (`initial_prompt`) en cada dictado. Lo orienta hacia
//!   esas palabras cuando suenan parecido, pero no garantiza que las escriba así: si la
//!   alternativa es una forma común del idioma ("de Marco" contra "Demarco"), gana ella.
//! - **Reemplazos**: cambios sobre el texto ya transcripto, que sí se aplican siempre.

use serde::{Deserialize, Serialize};

/// Un reemplazo del diccionario. En `reemplazar`, `\n` es un salto de línea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reemplazo {
    pub buscar: String,
    pub reemplazar: String,
}

/// Largo máximo de la pista. Whisper usa como mucho la mitad de su contexto de texto
/// (224 tokens) y descarta el principio si se pasa; con esto queda holgado.
pub const MAXIMO_CARACTERES: usize = 600;

/// La pista para Whisper: las palabras separadas por comas, sin repetidas ni vacías.
/// Si no entran todas, quedan las primeras.
pub fn pista(vocabulario: &[String]) -> Option<String> {
    let mut palabras: Vec<&str> = Vec::new();
    let mut largo = 0;
    for p in vocabulario
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
    {
        if palabras
            .iter()
            .any(|q| q.to_lowercase() == p.to_lowercase())
        {
            continue;
        }
        largo += p.chars().count() + 2;
        if largo > MAXIMO_CARACTERES {
            break;
        }
        palabras.push(p);
    }
    (!palabras.is_empty()).then(|| format!("{}.", palabras.join(", ")))
}

/// Si Whisper devolvió la pista misma: con audio casi vacío a veces la repite.
pub fn es_la_pista(texto: &str, pista: &str) -> bool {
    let normalizar = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let texto = normalizar(texto);
    !texto.is_empty() && texto == normalizar(pista)
}

/// Aplica los reemplazos en orden. Se buscan palabras o frases completas sin distinguir
/// mayúsculas: "polo" no toca "Polonia".
pub fn reemplazar(texto: &str, reemplazos: &[Reemplazo]) -> String {
    let mut s = texto.to_owned();
    for r in reemplazos {
        let buscar: Vec<char> = r.buscar.trim().chars().collect();
        if !buscar.is_empty() {
            s = reemplazar_uno(&s, &buscar, &r.reemplazar.replace("\\n", "\n"));
        }
    }
    ordenar_espacios(&s)
}

fn reemplazar_uno(texto: &str, buscar: &[char], por: &str) -> String {
    let t: Vec<char> = texto.chars().collect();
    let mut s = String::with_capacity(texto.len());
    let mut i = 0;
    while i < t.len() {
        if !coincide(&t, i, buscar) {
            s.push(t[i]);
            i += 1;
            continue;
        }
        i += buscar.len();
        if por.starts_with('\n') {
            // "Hola. Punto y aparte. Chau." → "Hola.\nChau.": sin el espacio de antes...
            while s.ends_with(' ') {
                s.pop();
            }
        }
        s.push_str(por);
        if por.is_empty() && s.trim_end().ends_with(',') {
            // "Quiero, o sea, que..." → "Quiero, que...": sin la coma repetida.
            let mut j = i;
            while j < t.len() && t[j] == ' ' {
                j += 1;
            }
            if t.get(j) == Some(&',') {
                i = j + 1;
            }
        }
        if por.ends_with('\n') {
            // ...ni la puntuación y el espacio que Whisper pone después.
            while i < t.len() && matches!(t[i], '.' | ',' | ';' | ':' | ' ') {
                i += 1;
            }
        }
    }
    s
}

/// Si `buscar` está en `t` a partir de `i`, como palabra completa.
fn coincide(t: &[char], i: usize, buscar: &[char]) -> bool {
    let fin = i + buscar.len();
    if fin > t.len() {
        return false;
    }
    let parecidas = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
    if !t[i..fin].iter().zip(buscar).all(|(&a, &b)| parecidas(a, b)) {
        return false;
    }
    // Solo se exige el borde si la búsqueda empieza o termina con una letra o número.
    let borde_antes = !buscar[0].is_alphanumeric() || i == 0 || !t[i - 1].is_alphanumeric();
    let borde_despues =
        !buscar[buscar.len() - 1].is_alphanumeric() || fin == t.len() || !t[fin].is_alphanumeric();
    borde_antes && borde_despues
}

/// Sin espacios dobles (quedan al borrar una frase) ni espacios pegados a un salto de línea.
fn ordenar_espacios(s: &str) -> String {
    s.split('\n')
        .map(|linea| {
            linea
                .split(' ')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches(' ')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regla(buscar: &str, reemplazar: &str) -> Reemplazo {
        Reemplazo {
            buscar: buscar.into(),
            reemplazar: reemplazar.into(),
        }
    }

    #[test]
    fn un_reemplazo_une_un_apellido_separado() {
        let r = [regla("de Marco", "Demarco")];
        assert_eq!(
            reemplazar("Mi nombre es Juan de Marco y estoy probando.", &r),
            "Mi nombre es Juan Demarco y estoy probando."
        );
        assert_eq!(reemplazar("Juan De Marco.", &r), "Juan Demarco.");
    }

    #[test]
    fn solo_se_reemplazan_palabras_completas() {
        let r = [regla("polo", "Polo Norte")];
        assert_eq!(reemplazar("Fui a Polonia.", &r), "Fui a Polonia.");
        assert_eq!(
            reemplazar("Me gusta el polo.", &r),
            "Me gusta el Polo Norte."
        );
        let r = [regla("ñu", "ÑU")];
        assert_eq!(reemplazar("Un ñu y un ñandú.", &r), "Un ÑU y un ñandú.");
    }

    #[test]
    fn punto_y_aparte_es_un_salto_de_linea() {
        let r = [regla("punto y aparte", "\\n")];
        assert_eq!(
            reemplazar("Hola. Punto y aparte. Chau.", &r),
            "Hola.\nChau."
        );
        assert_eq!(reemplazar("Hola, punto y aparte", &r), "Hola,\n");
    }

    #[test]
    fn un_reemplazo_vacio_borra_la_frase() {
        let r = [regla("o sea", "")];
        assert_eq!(
            reemplazar("Quiero, o sea, que funcione.", &r),
            "Quiero, que funcione."
        );
        let r = [regla("este", "")];
        assert_eq!(reemplazar("Bueno este vamos.", &r), "Bueno vamos.");
    }

    #[test]
    fn los_reemplazos_se_aplican_en_orden() {
        let r = [regla("uno", "dos"), regla("dos", "tres")];
        assert_eq!(reemplazar("uno", &r), "tres");
    }

    #[test]
    fn una_busqueda_vacia_no_hace_nada() {
        let r = [regla("  ", "x")];
        assert_eq!(reemplazar("Hola.", &r), "Hola.");
    }

    fn lista(palabras: &[&str]) -> Vec<String> {
        palabras.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn la_pista_junta_las_palabras_con_comas() {
        let v = lista(&["Wister", " Tauri ", "", "Svelte"]);
        assert_eq!(pista(&v).as_deref(), Some("Wister, Tauri, Svelte."));
    }

    #[test]
    fn sin_palabras_no_hay_pista() {
        assert_eq!(pista(&[]), None);
        assert_eq!(pista(&lista(&["  ", ""])), None);
    }

    #[test]
    fn las_repetidas_van_una_sola_vez() {
        let v = lista(&["Wister", "wister", "Tauri"]);
        assert_eq!(pista(&v).as_deref(), Some("Wister, Tauri."));
    }

    #[test]
    fn si_no_entran_todas_quedan_las_primeras() {
        let v: Vec<String> = (0..200).map(|i| format!("palabra{i}")).collect();
        let p = pista(&v).unwrap();
        assert!(p.chars().count() <= MAXIMO_CARACTERES);
        assert!(p.starts_with("palabra0, palabra1,"));
    }

    #[test]
    fn se_reconoce_cuando_whisper_repite_la_pista() {
        assert!(es_la_pista(
            "Wister, Tauri, Svelte.",
            "Wister, Tauri, Svelte."
        ));
        assert!(es_la_pista("wister tauri svelte", "Wister, Tauri, Svelte."));
        assert!(!es_la_pista("Probando Wister con Tauri.", "Wister, Tauri."));
        assert!(!es_la_pista("", "Wister."));
    }
}
