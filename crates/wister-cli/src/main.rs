//! Wister, Fase 0: prueba de concepto por consola.
//!
//! Graba del micrófono, transcribe localmente con Whisper y mide cuánto tarda.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use wister_core::audio::{self, Audio};
use wister_core::models::{self, CATALOG};
use wister_core::stt::{self, Engine, Options};

#[derive(Parser)]
#[command(
    name = "wister",
    version,
    about = "Dictado local con Whisper: prueba de concepto"
)]
struct Cli {
    /// Muestra los logs de whisper.cpp.
    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lista los micrófonos disponibles.
    Devices,
    /// Lista los modelos del catálogo y si están descargados.
    Models,
    /// Descarga un modelo del catálogo.
    Download {
        /// Nombre del modelo (ver `wister models`).
        model: String,
    },
    /// Graba del micrófono y transcribe.
    Dictate {
        #[command(flatten)]
        stt: SttArgs,
        #[command(flatten)]
        rec: RecordArgs,
        /// Repite el ciclo de grabar y transcribir hasta que cortes con Ctrl+C.
        #[arg(long)]
        repeat: bool,
    },
    /// Transcribe un archivo WAV.
    Transcribe {
        file: PathBuf,
        #[command(flatten)]
        stt: SttArgs,
    },
    /// Compara la latencia de varios modelos sobre el mismo audio.
    Bench {
        /// Modelos a comparar, separados por coma. Por defecto, todos los descargados.
        #[arg(long, value_delimiter = ',')]
        models: Vec<String>,
        /// WAV a usar. Si no se indica, se graba uno del micrófono.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Mediciones por modelo (después de una pasada de calentamiento).
        #[arg(long, default_value_t = 3)]
        runs: usize,
        #[command(flatten)]
        rec: RecordArgs,
        #[arg(long, default_value = "es")]
        lang: String,
        #[arg(long)]
        threads: Option<usize>,
        /// Fuerza CPU aunque el binario tenga soporte de GPU.
        #[arg(long)]
        cpu: bool,
    },
}

#[derive(Args)]
struct SttArgs {
    /// Modelo del catálogo o ruta a un `.bin` GGML.
    #[arg(short, long, default_value = "small")]
    model: String,
    /// Idioma ("es", "en", ...) o "auto".
    #[arg(short, long, default_value = "es")]
    lang: String,
    /// Texto que orienta a Whisper (nombres propios, jerga, estilo).
    #[arg(long)]
    prompt: Option<String>,
    #[arg(long)]
    threads: Option<usize>,
    /// Fuerza CPU aunque el binario tenga soporte de GPU.
    #[arg(long)]
    cpu: bool,
}

impl SttArgs {
    fn options(&self) -> Options {
        Options {
            language: self.lang.clone(),
            initial_prompt: self.prompt.clone(),
            threads: self.threads.unwrap_or_else(stt::default_threads),
        }
    }
}

#[derive(Args)]
struct RecordArgs {
    /// Parte del nombre del micrófono (ver `wister devices`).
    #[arg(short, long)]
    device: Option<String>,
    /// Graba esta cantidad de segundos en vez de esperar a Enter.
    #[arg(short, long)]
    seconds: Option<f32>,
    /// Guarda el audio grabado (16 kHz mono) en este WAV.
    #[arg(long)]
    save: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.verbose {
        stt::silence_native_logs();
    }
    match cli.command {
        Command::Devices => devices(),
        Command::Models => list_models(),
        Command::Download { model } => download(&model),
        Command::Dictate { stt, rec, repeat } => dictate(&stt, &rec, repeat),
        Command::Transcribe { file, stt } => transcribe_file(&file, &stt),
        Command::Bench {
            models,
            file,
            runs,
            rec,
            lang,
            threads,
            cpu,
        } => {
            let opts = Options {
                language: lang,
                initial_prompt: None,
                threads: threads.unwrap_or_else(stt::default_threads),
            };
            bench(models, file, runs.max(1), &rec, &opts, !cpu)
        }
    }
}

fn devices() -> Result<()> {
    let devices = audio::list_input_devices()?;
    if devices.is_empty() {
        println!("No se encontraron micrófonos.");
    }
    for d in devices {
        let mark = if d.is_default { " (por defecto)" } else { "" };
        println!("  {}{mark}", d.name);
    }
    Ok(())
}

fn list_models() -> Result<()> {
    println!("Carpeta: {}\n", models::models_dir()?.display());
    for m in CATALOG {
        let status = if m.is_downloaded() { "✔" } else { " " };
        println!(
            "  [{status}] {:<22} {:>5} MB  {}",
            m.name, m.size_mb, m.note
        );
    }
    Ok(())
}

fn download(name: &str) -> Result<()> {
    let model = models::find(name)?;
    if model.is_downloaded() {
        println!(
            "{} ya está descargado en {}",
            model.name,
            model.path()?.display()
        );
        return Ok(());
    }
    println!(
        "Descargando {} ({} MB) desde {}",
        model.name,
        model.size_mb,
        model.url()
    );
    let bar = ProgressBar::new(0).with_style(
        ProgressStyle::with_template(
            "{bar:40.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec} ETA {eta}",
        )
        .unwrap(),
    );
    let path = models::download(model, |done, total| {
        if let Some(total) = total {
            bar.set_length(total);
        }
        bar.set_position(done);
    })?;
    bar.finish_and_clear();
    println!("Listo, SHA-1 verificado: {}", path.display());
    Ok(())
}

fn load_engine(name_or_path: &str, use_gpu: bool) -> Result<Engine> {
    let path = models::resolve(name_or_path)?;
    eprint!("Cargando {}... ", path.display());
    let engine = Engine::load(&path, use_gpu).inspect_err(|_| eprintln!())?;
    eprintln!(
        "listo en {} ({})",
        ms(engine.load_time),
        if engine.gpu { "GPU" } else { "CPU" }
    );
    Ok(engine)
}

fn wait_enter(msg: &str) -> Result<()> {
    eprint!("{msg}");
    io::stderr().flush()?;
    let mut line = String::new();
    let n = io::stdin().lock().read_line(&mut line)?;
    if n == 0 {
        bail!("se cerró la entrada estándar");
    }
    Ok(())
}

/// Graba del micrófono hasta Enter o durante `rec.seconds`. Devuelve el audio a 16 kHz
/// y cuánto tardó el remuestreo (que en la app corre después de soltar la tecla).
fn record(rec: &RecordArgs) -> Result<(Audio, Duration)> {
    if rec.seconds.is_none() {
        wait_enter("Enter para empezar a grabar... ")?;
    }
    let recording = audio::start_recording(rec.device.as_deref())?;
    match rec.seconds {
        Some(secs) => {
            eprintln!("● Grabando {secs} s de \"{}\"...", recording.device_name);
            std::thread::sleep(Duration::from_secs_f32(secs));
        }
        None => wait_enter(&format!(
            "● Grabando de \"{}\". Enter para terminar... ",
            recording.device_name
        ))?,
    }
    let raw = recording.stop();

    let start = Instant::now();
    let audio = raw.to_whisper()?;
    let resample_time = start.elapsed();

    eprintln!(
        "  {:.1} s grabados a {} Hz, nivel RMS {:.3}",
        raw.duration_secs(),
        raw.sample_rate,
        raw.rms()
    );
    if raw.rms() < 0.002 {
        eprintln!("  ⚠ El audio está casi en silencio; revisá el micrófono con `wister devices`.");
    }
    if let Some(path) = &rec.save {
        audio.save_wav(path)?;
        eprintln!("  Guardado en {}", path.display());
    }
    Ok((audio, resample_time))
}

fn dictate(args: &SttArgs, rec: &RecordArgs, repeat: bool) -> Result<()> {
    let mut engine = load_engine(&args.model, !args.cpu)?;
    let opts = args.options();
    loop {
        let (audio, resample_time) = record(rec)?;
        let transcript = engine.transcribe(&audio, &opts)?;
        println!("\n{}\n", transcript.text);
        eprintln!(
            "  remuestreo {} + transcripción {} = {} desde que terminó la grabación (RTF {:.2})\n",
            ms(resample_time),
            ms(transcript.elapsed),
            ms(resample_time + transcript.elapsed),
            rtf(transcript.elapsed, &audio)
        );
        if !repeat {
            return Ok(());
        }
    }
}

fn transcribe_file(file: &std::path::Path, args: &SttArgs) -> Result<()> {
    let audio = Audio::load_wav(file)?.to_whisper()?;
    let mut engine = load_engine(&args.model, !args.cpu)?;
    let transcript = engine.transcribe(&audio, &args.options())?;
    println!("{}", transcript.text);
    eprintln!(
        "  {:.1} s de audio transcriptos en {} (RTF {:.2})",
        audio.duration_secs(),
        ms(transcript.elapsed),
        rtf(transcript.elapsed, &audio)
    );
    Ok(())
}

struct BenchRow {
    model: String,
    load: Duration,
    avg: Duration,
    min: Duration,
    rtf: f32,
    text: String,
}

fn bench(
    names: Vec<String>,
    file: Option<PathBuf>,
    runs: usize,
    rec: &RecordArgs,
    opts: &Options,
    use_gpu: bool,
) -> Result<()> {
    let names: Vec<String> = if names.is_empty() {
        CATALOG
            .iter()
            .filter(|m| m.is_downloaded())
            .map(|m| m.name.to_string())
            .collect()
    } else {
        names
    };
    if names.is_empty() {
        bail!("no hay modelos descargados; probá `wister download small`");
    }

    let audio = match &file {
        Some(path) => Audio::load_wav(path)
            .and_then(|a| a.to_whisper())
            .with_context(|| format!("no se pudo leer {}", path.display()))?,
        None => {
            let rec = RecordArgs {
                device: rec.device.clone(),
                seconds: Some(rec.seconds.unwrap_or(10.0)),
                save: rec.save.clone(),
            };
            eprintln!(
                "Vas a grabar el audio de prueba. Hablá normal, como si dictaras un mensaje."
            );
            record(&rec)?.0
        }
    };
    eprintln!(
        "\nAudio: {:.1} s · idioma {} · {} hilos · {} mediciones por modelo\n",
        audio.duration_secs(),
        opts.language,
        opts.threads,
        runs
    );

    let mut rows = Vec::new();
    for name in &names {
        let mut engine = load_engine(name, use_gpu)?;
        // La primera pasada reserva buffers y, en GPU, compila kernels: no se cuenta.
        let warmup = engine.transcribe(&audio, opts)?;
        let mut times = Vec::with_capacity(runs);
        for _ in 0..runs {
            times.push(engine.transcribe(&audio, opts)?.elapsed);
        }
        let avg = times.iter().sum::<Duration>() / runs as u32;
        let min = *times.iter().min().unwrap();
        eprintln!("  promedio {} · mínimo {}", ms(avg), ms(min));
        rows.push(BenchRow {
            model: format!("{name}{}", if engine.gpu { " (GPU)" } else { "" }),
            load: engine.load_time,
            avg,
            min,
            rtf: rtf(avg, &audio),
            text: warmup.text,
        });
    }

    println!("\n| Modelo | Carga | Promedio | Mínimo | RTF |\n|---|---:|---:|---:|---:|");
    for r in &rows {
        println!(
            "| {} | {} | {} | {} | {:.2} |",
            r.model,
            ms(r.load),
            ms(r.avg),
            ms(r.min),
            r.rtf
        );
    }
    println!();
    for r in &rows {
        println!("{}: {}", r.model, r.text);
    }
    Ok(())
}

fn ms(d: Duration) -> String {
    format!("{} ms", d.as_millis())
}

/// Real-time factor: tiempo de proceso / duración del audio. Menos de 1 es más rápido que tiempo real.
fn rtf(elapsed: Duration, audio: &Audio) -> f32 {
    elapsed.as_secs_f32() / audio.duration_secs().max(0.001)
}
