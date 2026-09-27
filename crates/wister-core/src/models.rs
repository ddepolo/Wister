//! Catálogo de modelos GGML de whisper.cpp: dónde se guardan, descarga y verificación.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use sha1::{Digest, Sha1};

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Copy)]
pub struct Model {
    /// Nombre corto que se usa en la CLI (`small`, `large-v3-turbo-q5_0`, ...).
    pub name: &'static str,
    pub size_mb: u32,
    /// SHA-1 publicado en `models/README.md` de whisper.cpp.
    pub sha1: &'static str,
    pub note: &'static str,
}

impl Model {
    pub fn file_name(&self) -> String {
        format!("ggml-{}.bin", self.name)
    }

    pub fn url(&self) -> String {
        format!("{BASE_URL}/{}", self.file_name())
    }

    pub fn path(&self) -> Result<PathBuf> {
        Ok(models_dir()?.join(self.file_name()))
    }

    pub fn is_downloaded(&self) -> bool {
        self.path().map(|p| p.is_file()).unwrap_or(false)
    }
}

/// Modelos multilingües (sirven para español). `tiny` no está a propósito: en las
/// pruebas en español transcribía demasiado mal. Los hashes vienen de
/// <https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md>.
pub const CATALOG: &[Model] = &[
    Model {
        name: "base",
        size_mb: 142,
        sha1: "465707469ff3a37a2b9b8d8f89f2f99de7299dac",
        note: "rápido pero se equivoca más; para PCs viejas",
    },
    Model {
        name: "small",
        size_mb: 466,
        sha1: "55356645c2b361a969dfd0ef2c5a50d530afd8d5",
        note: "buena calidad; el recomendado si no hay GPU",
    },
    Model {
        name: "large-v3-turbo-q5_0",
        size_mb: 547,
        sha1: "e050f7970618a659205450ad97eb95a18d69c9ee",
        note: "la mejor calidad por MB; el recomendado con GPU",
    },
    Model {
        name: "large-v3-turbo",
        size_mb: 1536,
        sha1: "4af2b29d7ec73d781377bfd1758ca957a807e941",
        note: "sin cuantizar: pesa el triple y casi no mejora a q5_0",
    },
];

pub fn find(name: &str) -> Result<&'static Model> {
    CATALOG.iter().find(|m| m.name == name).ok_or_else(|| {
        let names: Vec<_> = CATALOG.iter().map(|m| m.name).collect();
        anyhow!(
            "modelo desconocido \"{name}\"; los disponibles son: {}",
            names.join(", ")
        )
    })
}

/// `%LOCALAPPDATA%\Wister\models` en Windows, `~/.local/share/wister/models` en Linux.
/// Se puede cambiar con la variable de entorno `WISTER_MODELS_DIR`.
pub fn models_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("WISTER_MODELS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let dirs = directories::ProjectDirs::from("", "", "Wister")
        .context("no se pudo determinar la carpeta de datos del usuario")?;
    Ok(dirs.data_local_dir().join("models"))
}

/// Resuelve un modelo por nombre del catálogo o, si existe, por ruta a un `.bin`.
pub fn resolve(name_or_path: &str) -> Result<PathBuf> {
    let as_path = Path::new(name_or_path);
    if as_path.is_file() {
        return Ok(as_path.to_path_buf());
    }
    let model = find(name_or_path)?;
    let path = model.path()?;
    if !path.is_file() {
        bail!(
            "el modelo \"{}\" no está descargado; corré `wister download {}`",
            model.name,
            model.name
        );
    }
    Ok(path)
}

/// Descarga un modelo a la carpeta de modelos, reanudando si quedó una descarga a medias,
/// y verifica su SHA-1. `on_progress` recibe (bytes descargados, total si se conoce).
pub fn download(model: &Model, mut on_progress: impl FnMut(u64, Option<u64>)) -> Result<PathBuf> {
    let dest = model.path()?;
    if dest.is_file() {
        return Ok(dest);
    }
    fs::create_dir_all(dest.parent().unwrap())?;
    let part = dest.with_extension("bin.part");
    let already = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .user_agent(concat!("wister/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut request = client.get(model.url());
    if already > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={already}-"));
    }
    let mut response = request
        .send()
        .with_context(|| format!("no se pudo conectar a {}", model.url()))?
        .error_for_status()?;

    // Si el servidor ignora el Range, se empieza de cero.
    let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut downloaded = if resumed { already } else { 0 };
    let total = response.content_length().map(|len| len + downloaded);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&part)?;

    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    file.sync_all()?;
    drop(file);

    let actual = sha1_file(&part)?;
    if actual != model.sha1 {
        fs::remove_file(&part)?;
        bail!(
            "el SHA-1 de {} no coincide (esperado {}, obtenido {actual}); se borró la descarga",
            model.file_name(),
            model.sha1
        );
    }
    fs::rename(&part, &dest)?;
    Ok(dest)
}

pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_de_archivo_conocido() {
        let path = std::env::temp_dir().join(format!("wister-sha1-{}", std::process::id()));
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha1_file(&path).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn catalogo_consistente() {
        for m in CATALOG {
            assert_eq!(m.sha1.len(), 40, "{}", m.name);
            assert!(find(m.name).is_ok());
        }
        assert!(find("inexistente").is_err());
    }
}
