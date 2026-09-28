# Arquitectura

Cómo está armado Wister y por qué. Para compilar y medir, ver [`fase-0.md`](fase-0.md).

## Vista general

Wister es una app [Tauri 2](https://tauri.app): un proceso en Rust con dos ventanas web (WebView2). El trabajo pesado corre en hilos propios que se comunican por canales, y la interfaz se entera de todo por eventos de Tauri.

```
 ┌──────────────────────────────── Proceso wister-app (Rust) ─────────────────────────────────┐
 │                                                                                            │
 │  hilo wister-atajo              hilo wister-dictado                hilo wister-portapapeles│
 │  Raw Input ─▶ Detector ──────▶  (dueño del micrófono y del modelo) ───▶ portapapeles +     │
 │             (máquina de estados)  cpal ─▶ ring buffer ─▶ 16 kHz          Ctrl+V + restaurar │
 │                        Mensaje    has_voice ─▶ whisper.cpp ─▶ texto                        │
 │                                        │                                                   │
 │  comandos de la UI ──── Mensaje ──────▶│  (cambios de configuración, descargas, medidor)   │
 │                                        ▼                                                   │
 │                               eventos: estado, resultado, nivel                            │
 └────────────────────────────────────────┬───────────────────────────────────────────────────┘
                                          │ IPC de Tauri
                 ┌────────────────────────┴─────────────────────────┐
                 │ ventana "config": inicio, historial, diccionario,│
                 │   configuración y asistente                      │
                 │ ventana "overlay": pastilla con la onda, sin foco│
                 └──────────────────────────────────────────────────┘
```

## Estructura del repo

```
Cargo.toml                  workspace: versión, licencia y perfiles compartidos
crates/wister-core/         biblioteca sin Tauri, reutilizable
  src/audio.rs              captura (cpal + ring buffer), mono, remuestreo (rubato), WAV, has_voice
  src/models.rs             catálogo, descarga reanudable y verificación SHA-1
  src/stt.rs                motor whisper-rs y filtro de frases inventadas
  src/vad.rs                Silero VAD (modelo embebido en assets/)
crates/wister-cli/          bin `wister`: dictate, transcribe, bench, download...
src-tauri/                  la app
  src/main.rs               arranque, bandeja, ventanas, plugins y comandos
  src/hotkey.rs             Detector (máquina de estados del atajo) + Raw Input
  src/dictado.rs            hilo de dictado: modelo, grabación, transcripción, estado
  src/pegar.rs              portapapeles, Ctrl+V y restauración
  src/overlay.rs            mostrar/ocultar la pastilla sin activarla
  src/config.rs             config.json y los comandos de la ventana de configuración
  src/historial.rs          historial de dictados en SQLite y sus comandos
  src/sonidos.rs            tonos de inicio y fin generados en memoria
  tauri.conf.json           ventanas, bundle NSIS
src/                        interfaz en Svelte 5
  App.svelte                barra lateral y secciones (o el asistente)
  Inicio.svelte             estadísticas y últimos dictados
  Historial.svelte          buscador y lista del historial (también el resumen de Inicio)
  Diccionario.svelte        lugar reservado para el diccionario personal
  Configuracion.svelte      modelo, micrófono, idioma, atajo, preferencias e historial
  Icono.svelte              íconos de la barra lateral
  estilos.css               colores (claro y oscuro) y estilos compartidos
  Asistente.svelte          asistente de primer uso
  CapturaAtajo.svelte       captura de un atajo nuevo
  Overlay.svelte            la pastilla con la onda
  tipos.ts                  tipos que reflejan los Serialize de Rust
scripts/                    dev.ps1, build.ps1, logo.py
```

## El dictado, paso a paso

1. **Apretar el atajo.** Raw Input entrega cada tecla al hilo `wister-atajo`, que alimenta al `Detector`. Cuando están todas las teclas del atajo abajo, emite `Inicio`.
2. **Grabar.** El hilo `wister-dictado` abre el micrófono al instante (tarda unos 20 ms), así que no se pierde el principio.
3. **Confirmar a los 300 ms.** El overlay, el estado "Grabando" y el sonido de inicio esperan a que el atajo lleve 300 ms apretado. Si antes se toca otra tecla, fue un atajo común (`Ctrl+Shift+T`) y no aparece nada.
4. **Soltar.** El `Detector` emite `Fin`, o `Cancelado` si se tocó otra tecla o fue un toque corto. Si se cancela, lo grabado se descarta.
5. **Filtrar.** Si el audio no tiene energía de voz (`Audio::has_voice`), no se sigue.
6. **Detectar la voz.** Se remuestrea a 16 kHz y Silero VAD deja solo los tramos con voz. Si no hay ninguno, no se transcribe.
7. **Transcribir.** whisper.cpp transcribe con el modelo que ya está en memoria. Si el resultado es solo una frase de las que Whisper inventa ("Gracias."), se descarta.
8. **Pegar.** El hilo `wister-portapapeles` guarda el portapapeles, pone el texto, manda `Ctrl+V` y a los 300 ms restaura lo que había.
9. **Informar.** El resultado va a la interfaz (evento `resultado`) y el estado vuelve a "En espera", lo que oculta el overlay.
10. **Anotar.** Si el historial está activado, el texto se guarda en SQLite con el título de la ventana que estaba activa al soltar el atajo. Va después de ocultar el overlay, para que la escritura no lo demore.

## Módulos

### Atajo (`hotkey.rs`)

`RegisterHotKey` (lo que usa `tauri-plugin-global-shortcut`) no sirve: no avisa cuándo se **suelta** un atajo de solo modificadores y no distingue izquierda de derecha.

Wister escucha el teclado con **Raw Input** (`RegisterRawInputDevices` con `RIDEV_INPUTSINK` sobre una ventana oculta):

- **Por qué no un hook `WH_KEYBOARD_LL`**: fue la primera versión. Los hooks forman una cadena, y un programa con un hook más nuevo que no llama a `CallNextHookEx` deja a los demás sin teclas. En una PC de prueba, algún programa instalado se comía `Ctrl` y `Shift`, y el atajo dejaba de andar sin aviso. Raw Input recibe una copia de lo que llega al sistema y nadie puede interponerse.
- Raw Input informa `VK_SHIFT`/`VK_CONTROL` sin lado: el lado sale del scan code (Shift) o del prefijo E0 (Ctrl, Alt). Se descartan las teclas "falsas" que genera el teclado (VKey 0xFF, y los Shift con E0 que acompañan a las flechas con Bloq Num).
- Las teclas inyectadas con `SendInput`, como el propio `Ctrl+V` de Wister, llegan sin dispositivo (`hDevice` nulo) y se ignoran.
- Windows admite un registro de Raw Input por tipo de dispositivo y por proceso. El de Wister reemplaza el del teclado que hace tao (el runtime de Tauri), que solo sirve para eventos de dispositivo que no se usan.
- Las teclas **no se tragan**: le siguen llegando a la app activa. Por eso tocar otra tecla con el atajo apretado cancela el dictado, y así los atajos comunes siguen funcionando.
- Atajos permitidos: `Ctrl`, `Shift` y `Alt` de cada lado, y F1–F24. `Win` no, porque Raw Input solo observa y no puede impedir que abra el menú Inicio (para eso haría falta un hook que inyecte `VK_NONE`, como AutoHotkey). Las letras tampoco, porque se escribirían mientras se dicta.

El `Detector` es una máquina de estados sin nada de Win32 y con tests: `Reposo → Activo → (Fin | Cancelado)`, ignorando las repeticiones automáticas de las teclas.

### Audio (`wister-core/src/audio.rs`)

- `cpal` abre el micrófono elegido, o el predeterminado, en su formato nativo (normalmente 48 kHz).
- El callback corre en el hilo de audio del sistema, que no puede reservar memoria ni bloquearse: solo mezcla a mono y escribe en un ring buffer sin locks (`ringbuf`, 2 s). Un hilo lo vacía cada 10 ms e informa el nivel RMS cada 50 ms para la onda del overlay.
- Al terminar se remuestrea a 16 kHz con `rubato` (FFT sincrónica).
- **Filtro de energía**: `has_voice` pide al menos 200 ms en ventanas de 20 ms con RMS mayor a 0,01. Un RMS global no sirve porque las pausas diluyen la voz. Es instantáneo y descarta el silencio total antes de gastar nada más.

### Detección de voz (`wister-core/src/vad.rs`)

Whisper **siempre** inventa algo con audio sin voz ("Gracias.", "¡Suscríbete!", restos de los subtítulos con los que se entrenó), y el filtro de energía deja pasar ruidos fuertes (tos, teclado, golpes). Por eso, antes de transcribir, pasa por **Silero VAD**:

- Se queda solo con los tramos con voz (con 100 ms de margen para no comerse sílabas) y los une con 100 ms de silencio. Menos audio para Whisper y menos texto inventado al final de la frase.
- Si no hay tramos con voz, no se transcribe.
- whisper.cpp puede aplicar el VAD dentro de `whisper_full`, pero whisper-rs usa `whisper_full_with_state`, que lo ignora. Por eso se usa la API independiente (`WhisperVadContext`).
- El modelo (`ggml-silero-v6.2.0.bin`, 865 KB, MIT) viaja dentro del binario con `include_bytes!` y se escribe en la carpeta de modelos la primera vez, porque whisper.cpp solo lo lee de un archivo. Así funciona desde el primer arranque y sin red.
- Corre en CPU con **un solo hilo**: para 10 s de audio tarda ~20 ms. Con 2 hilos tarda ~80 ms y con 4, ~180 ms, porque coordinarlos cuesta más de lo que ahorran.

Si al final el texto es solo una de las frases típicas (`stt::is_hallucination`), se descarta. Si aparece dentro de una frase real ("Gracias por la ayuda"), se deja.

### Transcripción (`wister-core/src/stt.rs`, `dictado.rs`)

- El modelo se carga una vez al arrancar, o al cambiarlo desde la configuración, y queda en memoria. Al cambiarlo se libera el anterior antes de cargar el nuevo, para no ocupar la memoria de la GPU dos veces.
- **Calentamiento**: la primera transcripción con Vulkan compila shaders (unos 7 s la primera vez; después el driver los cachea). Al cargar el modelo se transcribe 1 s de silencio para que no lo pague el primer dictado.
- Sampling greedy (`best_of: 1`): para dictado alcanza y es bastante más rápido que beam search.
- Con menos de 1 s de audio Whisper tiende a inventar, así que se completa con silencio hasta 1 s.
- Backends: CPU (con AVX2), **Vulkan** (NVIDIA, AMD, Intel) y CUDA como opción de compilación. Vulkan es el de por defecto porque las DLL de CUDA pesan cientos de MB.
- El modelo recomendado es `large-v3-turbo-q5_0` si el binario tiene GPU y `small` si no. Hoy se decide según cómo se compiló, no según si la PC tiene una GPU compatible.

### Pegado (`pegar.rs`)

1. Esperar (hasta 1,5 s) a que no quede ningún modificador apretado, para que el `Ctrl+V` no salga como `Ctrl+Shift+V`.
2. Si la ventana activa corre como administrador, no pegar: por UIPI Windows descarta el `SendInput` sin avisar. El texto queda en el portapapeles y la interfaz lo informa.
3. Guardar todos los formatos del portapapeles que sean memoria global (texto, imágenes DIB, HTML…). Los objetos GDI y privados se saltean: Windows sintetiza `CF_BITMAP` a partir de `CF_DIB`.
4. Poner el texto marcado con `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory = 0` y `CanUploadToCloudClipboard = 0`, para que no aparezca en `Win+V` ni se sincronice.
5. `SendInput`: `Ctrl` abajo, `V` abajo, `V` arriba, `Ctrl` arriba.
6. A los 300 ms, restaurar lo guardado (con las mismas marcas), **solo si** nadie cambió el portapapeles mientras tanto (`GetClipboardSequenceNumber`). La app destino lee el portapapeles cuando procesa el `Ctrl+V`, no cuando lo recibe.

El portapapeles lo maneja un hilo propio con una ventana `HWND_MESSAGE` que procesa mensajes cada 10 ms. Windows le manda mensajes **sincrónicos** al dueño del portapapeles (por ejemplo, `WM_DESTROYCLIPBOARD` cuando otra app lo vacía), y si ese hilo no los atiende, la otra app se cuelga esperando.

Si un dictado va a la misma ventana que el anterior antes de 60 s, se le antepone un espacio (salvo que empiece con puntuación). Saber qué hay antes del cursor requeriría UI Automation.

### Overlay (`overlay.rs`, `Overlay.svelte`)

- Ventana transparente, sin bordes, siempre visible, que deja pasar los clics.
- **Nunca puede quedarse con el foco**, porque el `Ctrl+V` iría a parar al overlay. Lleva `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` y se muestra y oculta con `ShowWindow` nativo (`SW_SHOWNOACTIVATE` / `SW_HIDE`). El `show()` de Tauri activa la ventana, y como Tauri no se entera de que se mostró por fuera, su `hide()` no hace nada.
- Se ubica abajo al centro del área de trabajo del monitor donde está el mouse.

### Configuración (`config.rs`)

- `config.json` en `%APPDATA%\ar.wister.app\`. Se guarda en cada cambio, y el hilo de dictado recibe la configuración nueva por su canal.
- Los campos que faltan toman su valor por defecto (`#[serde(default)]`), así una configuración vieja sigue sirviendo. Un modelo o atajo que ya no es válido vuelve al de por defecto.
- Tauri crea las ventanas de `tauri.conf.json` **antes** del `setup`, y en release la interfaz embebida carga tan rápido que llama a los comandos antes de que el `setup` registre su estado. Por eso los comandos usan `try_state` para lo que se registra en el `setup`.
- **Instancia única** (`tauri-plugin-single-instance`): abrir Wister de nuevo muestra la ventana de la instancia que ya corre, en vez de sumar otra que escucharía el atajo y pegaría cada dictado dos veces.

### Historial (`historial.rs`, `Historial.svelte`)

- SQLite embebido (`rusqlite` con `bundled`: no depende de un SQLite instalado) en `%APPDATA%\ar.wister.app\historial.db`. Tabla `dictados`: fecha (ms UTC), texto, duración del audio, palabras y título de la ventana destino.
- El esquema se versiona con `PRAGMA user_version`: `MIGRACIONES` es una lista y al abrir se aplican las que faltan.
- La conexión se abre la primera vez que se usa; el estado se registra antes del `setup`, así que los comandos usan `State`.
- `secure_delete` para que lo borrado no quede en el archivo, y "Borrar todo" hace `VACUUM`. WAL con `synchronous = NORMAL`: cada dictado no espera al disco (un corte de luz puede perder el último, pero no corrompe la base).
- La búsqueda usa una función SQL propia, `plegar` (minúsculas y sin tildes; la `ñ` se deja), con `instr` en vez de `LIKE` para no tener que escapar `%` y `_`.
- La interfaz se actualiza con el evento `historial` (`agregado`, `borrado`, `borrado_todo`, `error`) sin volver a pedir la lista.
- **Estadísticas** (comando `estadisticas`): una consulta agrupa los dictados por día del calendario local (`date(..., 'localtime')`) y cuenta cuántos días atrás es cada uno; el resto se calcula en Rust (`resumir`, con tests). El tiempo ahorrado es lo que se tardaría tipeando a 40 palabras por minuto menos lo que duró el audio. La racha cuenta días seguidos hasta hoy o hasta ayer, para que no se corte antes del primer dictado del día.

### Ventana principal (`App.svelte`)

- Barra lateral con Inicio, Historial, Diccionario y Configuración, el estado del dictado y "Salir de Wister" (con confirmación: cerrar la ventana solo la esconde en la bandeja).
- Las secciones quedan montadas y se ocultan con `hidden`: así no se pierden la búsqueda del historial ni el progreso de una descarga al cambiar de sección.
- Bandeja: doble clic abre la ventana; el menú (Abrir Wister, Salir) va solo en el clic derecho, porque con el izquierdo el primer clic abriría el menú y el doble clic no llegaría.
- El logo se importa con `?no-inline`: si Vite lo embebiera como `data:`, la CSP no lo dejaría cargar.

### Modelos (`wister-core/src/models.rs`)

- Catálogo embebido con nombre, tamaño y el SHA-1 que publica whisper.cpp en su `models/README.md`.
- Descarga reanudable (`.part` + header `Range`) desde Hugging Face, sin timeout, y verificación del hash al terminar.
- `tiny` no está: en español transcribe demasiado mal.
- Es el único código que usa la red.

## Compilación en Windows

Dos trampas que resuelven `scripts/dev.ps1` y `scripts/build.ps1`:

- **whisper.cpp sin `/O2`**: el crate `cmake` reemplaza `CMAKE_C_FLAGS_RELEASE` por los flags de `cc`, que en MSVC no traen `/O2`, y whisper.cpp queda sin optimizar: unas 8 veces más lento en CPU. Se arregla definiendo `CMAKE_C_FLAGS_RELEASE` y `CMAKE_CXX_FLAGS_RELEASE` como `/MD /O2 /Ob2 /DNDEBUG`, que whisper-rs-sys le pasa a CMake. Después de cambiarlas hay que correr `cargo clean -p whisper-rs-sys`.
- **Rutas largas con Vulkan**: el subproyecto `vulkan-shaders-gen` queda tan anidado dentro de `target\` que MSBuild supera los 260 caracteres (`FTK1011`, `MSB8066`). Se compila en `C:\wt` (desarrollo) y `C:\wr` (release).
- **Instrucciones de CPU**: `build.ps1` compila con `GGML_NATIVE=OFF` y AVX2, también con Vulkan (sin GPU se usa el procesador). `dev.ps1` usa las nativas, y por eso cada uno tiene su target: whisper.cpp no se recompila solo si cambian esas variables.

## Publicar una versión

1. Subir la versión en `Cargo.toml` y `package.json`, y pasar lo de "Sin publicar" del `CHANGELOG.md` a su sección.
2. `.\scripts\release.ps1`: compila los dos instaladores y deja en `C:\wr\publicar\v<versión>\` los `.exe`, `latest.json`, `notas.md` y `SHA256SUMS.txt`.
3. Tag `v<versión>`, push, y `gh release create v<versión> --title "Wister <versión>" --notes-file notas.md` con esos archivos.

### Actualizaciones (`actualizar.rs`)

- `tauri-plugin-updater`, solo a pedido: el botón de Configuración llama a `buscar_actualizacion` y a `instalar_actualizacion`. El endpoint es `releases/latest/download/latest.json`, así que siempre apunta al último release.
- Cada variante busca su entrada en `latest.json`: `windows-x86_64` (Vulkan) y `windows-x86_64-cpu` (`UpdaterBuilder::target`), para que la de CPU no se pase a la de Vulkan.
- Los instaladores se firman con una clave de Tauri (minisign), distinta de la firma de código de Windows: la privada está en `%USERPROFILE%\.tauri\wister-actualizaciones.key`, fuera del repo, y la pública en `tauri.conf.json`. **Si se pierde, las versiones instaladas no pueden verificar las nuevas.** `requireSignedVersion` exige que la firma incluya la versión, para que no se pueda forzar la vuelta a una versión vieja.
- Para probar sin publicar: `release.ps1 -Prueba` compila apuntando a `http://127.0.0.1:8765` (`scripts/actualizacion-prueba.json`). Se instala esa versión, se sube el número, se vuelve a correr y se sirve la carpeta con `python -m http.server 8765`.

En desarrollo, las dependencias y `wister-core` se compilan optimizadas (`[profile.dev.package]` en el `Cargo.toml`): los genéricos de rubato se instancian en `wister-core`, y sin optimizar el remuestreo de 12 s de audio tardaba 289 ms (optimizado, 5 ms).

## Rendimiento

Medido en un Intel Core Ultra 7 265K con una RTX 5070 Ti (detalle en [`fase-0.md`](fase-0.md)):

| | 10 s de audio (bench) | En la app, al soltar el atajo |
|---|---:|---:|
| `large-v3-turbo-q5_0`, GPU (Vulkan) | ~80 ms | ~150–350 ms |
| `small`, CPU (8–16 hilos) | ~1,2–1,6 s | — |

La diferencia entre el bench y la app todavía no está explicada. Puede ser que la GPU baje los relojes en reposo entre dictados.

## Privacidad

- Sin telemetría ni reportes automáticos de errores.
- El audio se procesa en memoria y no se guarda.
- El historial guarda solo el texto, en la PC, y se puede desactivar o borrar.
- La red se usa solo para bajar modelos, a pedido.

## Riesgos y preguntas abiertas

- **Antivirus**: `SendInput` y la lectura global del teclado pueden disparar falsos positivos. La mitigación es firmar el binario (hay firma gratuita para proyectos open source, como SignPath) y publicar el código.
- **Detección de GPU**: el modelo recomendado depende de cómo se compiló el binario. Un build con Vulkan en una PC sin GPU compatible recomendaría `turbo`, que en CPU es lento.
- **Streaming**: Whisper no transcribe en vivo. Hoy se transcribe todo al soltar el atajo.
