# CLAUDE.md

Contexto del proyecto para Claude Code (y para cualquiera que se sume). Leelo entero antes de tocar código. El diseño detallado, con los porqués, está en [`docs/arquitectura.md`](docs/arquitectura.md).

## Qué es Wister

Dictado por voz **libre y local** para **Windows**: mantenés apretado un atajo (`Ctrl` + `Shift` izquierdos por defecto), hablás, soltás, y el texto transcripto se pega en la app que tiene el foco. La transcripción corre en la PC con Whisper (whisper.cpp), sin nube, sin cuentas y sin telemetría.

Principios que no se negocian:

- **Todo local**: el audio y el texto no salen de la PC.
- **Red solo a pedido**: hoy, únicamente para bajar modelos cuando el usuario lo pide.
- **Licencia GPL-3.0-or-later** (`LICENSE`): cualquier derivado tiene que seguir siendo libre.

## Convenciones

- **Idioma**: la documentación, los comentarios, los mensajes de error y la interfaz van en **español rioplatense** (voseo: "tenés", "corré", "fijate"). Los **mensajes de commit van en inglés**.
- **Plataforma**: Windows primero. macOS y Linux quedan para más adelante, pero el código no debería cerrarles la puerta: lo específico de Windows va detrás de `#[cfg(windows)]` y el resto compila en Linux (CI lo verifica).
- Errores con `anyhow` y `.context("mensaje en español")`. Los mensajes para el usuario dicen qué hacer ("descargalo en Configuración").
- Tests unitarios en el mismo archivo (`#[cfg(test)] mod tests`), con nombres en español que describen el comportamiento (`un_toque_corto_se_descarta`). La lógica se separa de Win32 para poder testearla (por ejemplo, `hotkey::Detector`).
- Comentarios solo donde explican un *por qué* que no es obvio.
- Avanzar en pasos chicos y verificables. Lo que no se puede testear automáticamente (atajo, pegado, overlay) se prueba en la app antes de dar un paso por terminado.

## Decisiones tomadas (no volver a discutirlas sin motivo)

| Tema | Decisión | Por qué |
|---|---|---|
| Stack | **Tauri 2 + Rust**, con la UI en **Svelte 5 + TypeScript** (Vite) | Binario nativo liviano, whisper.cpp embebido, sin Python ni servidor aparte; la UI es chica y Svelte da el bundle más liviano |
| Motor STT | **whisper.cpp vía `whisper-rs`**, modelos GGML | Corre en CPU y GPU |
| GPU | **Vulkan** por defecto; CUDA como opción de compilación | Vulkan funciona en NVIDIA, AMD e Intel; las DLL de CUDA pesan cientos de MB |
| Atajo global | **Raw Input** (`RIDEV_INPUTSINK`), no `RegisterHotKey`, `tauri-plugin-global-shortcut` ni hook `WH_KEYBOARD_LL` | Hay que detectar el *soltado* de atajos de solo modificadores y distinguir izquierda/derecha. El hook se descartó porque cualquier programa con un hook más nuevo puede dejarlo sin teclas |
| Atajo por defecto | **`Ctrl` + `Shift` izquierdos**; configurable con `Ctrl`/`Shift`/`Alt` y F1–F24 | No abre el menú Inicio. Las teclas no se tragan: tocar otra tecla cancela (así `Ctrl+Shift+T` sigue andando) y un toque de menos de 300 ms se descarta. `Win` no se permite: Raw Input no puede evitar que abra Inicio |
| Insertar texto | Portapapeles + `Ctrl+V` con `SendInput`, restaurando el contenido anterior y excluyéndolo del historial de `Win+V` | Rápido y respeta Unicode |
| Overlay | Ventana Tauri transparente con `WS_EX_NOACTIVATE`, mostrada con `ShowWindow` nativo | No puede robarle el foco a la app destino |
| Verificación de modelos | **SHA-1** publicado en `models/README.md` de whisper.cpp | Es el hash oficial que existe; no inventar otros |
| Ventana del codificador | **30 s completos** (sin `audio_ctx`) | Achicarla al largo del audio era 2,4–3,2× más rápido en CPU, pero con `turbo` escribía cualquier cosa y en GPU compilaba shaders por tamaño. Descartado |
| Sampling | Greedy (`best_of: 1`) | Para dictado alcanza y es bastante más rápido que beam search |
| Modelos | `base`, `small`, `large-v3-turbo-q5_0`, `large-v3-turbo`; **sin `tiny`** | `tiny` transcribe muy mal en español. Recomendado: `turbo-q5_0` con GPU, `small` sin GPU |

## Estado

**Versión 0.3.0**: dictado push-to-talk completo (atajo, grabación, transcripción, pegado) con VAD, overlay, ventana con barra lateral (Inicio con estadísticas, Historial en SQLite, Diccionario con vocabulario y reemplazos, Configuración), asistente de primer uso, micrófono y su volumen (también desde la bandeja), registro de funcionamiento, prueba de rendimiento y exportar diagnóstico, actualización con un botón y dos instaladores NSIS (Vulkan y solo CPU) en GitHub Releases. El detalle está en `CHANGELOG.md`.

Cancelar un dictado con `Esc` ya funciona: cualquier otra tecla apretada mientras se graba lo cancela.

Pendiente para la 0.4.0, en orden aproximado de prioridad:

Hecho sin publicar (ver `CHANGELOG.md`): despertar la GPU mientras se habla, copiar el último dictado desde la bandeja, "No poner punto al final", comandos de voz listos y el aviso de RAM al elegir un modelo.

1. **Tema claro / oscuro / automático** en Configuración (hoy sigue el modo de apps de Windows).
2. **Idioma en la bandeja**, como el micrófono.
3. **Modo manos libres**: tocar el atajo para empezar y otra vez para terminar. Toca `hotkey::Detector`, que es delicado.
4. **Instaladores compilados en GitHub Actions y firmados** con SignPath Foundation (gratis para proyectos libres; firma solo lo compilado en CI). Arrancar la postulación temprano, porque tarda.

En espera: elegir CPU o GPU a mano. En la notebook de prueba la GPU integrada fue 2,6 veces más rápida que la CPU, así que por ahora no hace falta; se retoma si la prueba de rendimiento muestra un caso contrario.

Más adelante: post-procesado con un LLM local, estilos por app, macOS y Linux.

## Estructura

```
Cargo.toml                 workspace (versión, licencia y perfiles compartidos)
crates/wister-core/        lib: audio.rs, models.rs, stt.rs, vad.rs (sin Tauri; el modelo de VAD está en assets/)
crates/wister-cli/         bin `wister`: CLI para probar y medir
src-tauri/                 app Tauri (bin `wister-app`): hotkey, dictado, pegar, overlay, bandeja, config, diccionario, volumen, historial, sonidos, registro, sistema, rendimiento, diagnostico
src/                       UI en Svelte 5: App (barra lateral), Inicio, Historial, Diccionario, Configuracion, Rendimiento, Asistente, Overlay; estilos.css y tipos.ts
scripts/                   dev.ps1, build.ps1, logo.py
docs/arquitectura.md       diseño, módulos y decisiones
docs/fase-0.md             compilación, CLI y mediciones de modelos
.github/workflows/ci.yml   Linux: fmt, clippy -D warnings, tests · Windows: tests, build de la CLI
```

## Comandos

```powershell
npm install                     # una vez
.\scripts\dev.ps1               # app en modo desarrollo con Vulkan (-Cpu para solo CPU)
.\scripts\build.ps1             # instalador NSIS en C:\wr\release\bundle\nsis\ (-Cpu para solo CPU)
.\scripts\release.ps1           # los dos instaladores + latest.json en publicar\v<versión> (-Prueba)
npm run check                   # svelte-check
cargo fmt
cargo clippy --all-targets -- -D warnings   # CI falla con cualquier warning
cargo test
```

Antes de pushear: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` y `npm run check`.

Fuera de los scripts, para compilar whisper.cpp en Windows hacen falta las variables que arman `dev.ps1`/`build.ps1` (ver "Notas técnicas"). La CLI se usa así: `cargo build --release -p wister-cli --features vulkan` y después `wister bench --file audio.wav` (más en `docs/fase-0.md`).

Requisitos en Windows: Rust (MSVC), Visual Studio Build Tools (C++), CMake, LLVM, Node.js 24+ y, para GPU, el Vulkan SDK. En Linux (CI): `libasound2-dev` para cpal y, para Tauri, `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`. `tauri::generate_context!` necesita el frontend compilado en `dist/` (`npm run build`).

## Notas técnicas

Trampas que ya costaron tiempo:

- **whisper.cpp sin `/O2` en MSVC**: el crate `cmake` reemplaza `CMAKE_C_FLAGS_RELEASE` por los flags de `cc`, que no traen `/O2` (~8x más lento en CPU). Se define `CMAKE_C_FLAGS_RELEASE` y `CMAKE_CXX_FLAGS_RELEASE` = `/MD /O2 /Ob2 /DNDEBUG` (whisper-rs-sys le pasa a CMake cualquier `CMAKE_*`, `GGML_*` o `WHISPER_*`). Después de cambiarlas: `cargo clean -p whisper-rs-sys`, porque el build script no se vuelve a ejecutar solo. Falta ver si en Linux pasa lo mismo con `-O3`.
- **Vulkan y MAX_PATH**: `vulkan-shaders-gen` queda tan anidado en `target\` que MSBuild falla (`FTK1011`, `MSB8066`). Se usa `CARGO_TARGET_DIR=C:\wt`. Si se corta una compilación de Vulkan a la mitad puede quedar un shader generado truncado (`error C1075` en `mul_mm.comp.cpp`): `cargo clean -p whisper-rs-sys` con ese target.
- **CPU portable**: con `GGML_NATIVE=OFF` + `GGML_AVX/AVX2/FMA/F16C=ON` el binario funciona en cualquier x64 con AVX2 y no solo en la CPU donde se compiló (CI y `build.ps1 -Cpu`).
- **Perfil dev**: las dependencias y `wister-core` van con `opt-level = 3`, porque los genéricos de rubato se instancian en `wister-core` (en debug el remuestreo de 12 s tardaba 289 ms; optimizado, 5 ms).
- **tauri dev**: cortar la app mientras el watcher recompila puede dejar artefactos mezclados (`LNK2019` con símbolos `anon.*.llvm`): `cargo clean -p wister-core -p wister-app` en ese target. Antes de relanzar, cortar la app, los `cargo`/`rustc` que queden y lo que escuche en el puerto 1420.
- **Actualizaciones**: se firman con la clave de `%USERPROFILE%\.tauri\wister-actualizaciones.key` (fuera del repo; `build.ps1` la toma de ahí). Si se pierde, las versiones instaladas no pueden verificar las nuevas. `latest.json` tiene una entrada por variante (`windows-x86_64` y `windows-x86_64-cpu`); el proceso para publicar está en `docs/arquitectura.md`.
- **Targets**: `dev.ps1` compila en `C:\wt` con las instrucciones nativas de la CPU; `build.ps1`, en `C:\wr` (Vulkan) y `C:\wt-cpu`, con AVX2 fijo. No mezclarlos: whisper.cpp no se recompila solo si cambian las variables `GGML_*`.
- **Build release**: no redirigir la salida de `build.ps1` con `2>&1`, porque PowerShell 5.1 convierte el stderr de node en un error y `$ErrorActionPreference = "Stop"` corta el script.
- **Estado de Tauri**: las ventanas de `tauri.conf.json` se crean antes del `setup`, y en release la UI llama a los comandos antes de que exista el estado que registra el `setup`. Esos comandos usan `try_state`.
- **COM**: `windows-sys` no trae las interfaces COM; para `IAudioEndpointVolume` se usa el crate `windows` 0.62 (el que ya trae Tauri), en un hilo propio con `COINIT_MULTITHREADED`.
- **Overlay**: `show()` de Tauri activa la ventana y, si se muestra por fuera, su `hide()` no hace nada. Se usa `ShowWindow` nativo para las dos cosas.
- **Raw Input**: un solo registro por tipo de dispositivo y por proceso; el de Wister reemplaza el del teclado de tao. Las teclas inyectadas llegan con `hDevice` nulo.
- **Portapapeles**: el hilo dueño tiene que procesar mensajes siempre (Windows le manda `WM_DESTROYCLIPBOARD` de forma sincrónica). `SendInput` no avisa cuando UIPI lo bloquea: se revisa `TokenElevation` de la ventana activa.
- **Confirmación a los 300 ms**: la grabación arranca al apretar, pero el overlay, el estado "Grabando" y el sonido esperan `DURACION_MINIMA` (el hilo de dictado usa `recv_timeout`).
- **VAD**: whisper.cpp solo aplica su VAD en `whisper_full`, y whisper-rs usa `whisper_full_with_state`, que lo ignora (activarlo en `FullParams` no hace nada). Se usa `WhisperVadContext` aparte (`vad.rs`), con **un solo hilo** (con más es más lento). Silero v6.2.0 viene embebido con `include_bytes!` (excepción en `.gitignore`).
- **whisper-rs 0.16**: `WhisperState` guarda un `Arc` al contexto, así que alcanza con guardar el estado; crearlo es caro y se reutiliza. Con menos de 1 s de audio Whisper inventa: `Engine::transcribe` completa con silencio. La primera transcripción con Vulkan compila shaders (~7 s): se "calienta" al cargar el modelo.
- **rubato 5**: `Fft::new(in, out, 1024, 1, FixedSync::Input)` + `process_all` + `take_data()` devuelve exactamente la longitud esperada.
- **cpal 0.18**: el nombre del dispositivo sale de `device.to_string()`; la selección es por subcadena, sin distinguir mayúsculas.
- **rusqlite**: no implementa `ToSql`/`FromSql` para `u64` ni `usize` (sin features extra): se usan `u32`/`i64`. Guardar en el historial va después de ocultar el overlay, y la base va en WAL con `synchronous = NORMAL` para que cada `INSERT` no espere al disco.
- **reqwest blocking**: tiene 30 s de timeout por defecto; para descargar modelos se desactiva con `.timeout(None)`.
- **Registro**: se usa `log::info!`/`warn!`/`error!`, no `eprintln!` (en release no hay consola y se pierde). Nunca anotar el texto dictado. Los mensajes de whisper.cpp llegan por `log` (feature `log_backend`); los informativos solo se anotan dentro de `registro::con_detalle_nativo`, filtrados por `nativo_util`.
- Rutas: modelos en `%LOCALAPPDATA%\Wister\data\models` (o `WISTER_MODELS_DIR`), configuración en `%APPDATA%\ar.wister.app\config.json` e historial en `historial.db`, en la misma carpeta. El registro, en `%LOCALAPPDATA%\ar.wister.app\logs\wister.log`. Los comparten la app instalada, la de desarrollo y la CLI.

## Riesgos conocidos

- **UIPI**: no se puede mandar input a ventanas elevadas; el texto queda en el portapapeles y se avisa.
- **Antivirus**: `SendInput` y la lectura global del teclado pueden dar falsos positivos. Hay que firmar el binario.
- Whisper no es streaming: se transcribe al soltar el atajo.
