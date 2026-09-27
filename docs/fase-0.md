# Fase 0: prueba de concepto

> Este documento es de la primera etapa del proyecto, antes de la app. Sigue siendo la referencia para compilar la CLI, medir modelos con `wister bench` y las mediciones que definieron el modelo por defecto.

El objetivo de esta fase es comprobar que la cadena **micrófono → 16 kHz → whisper.cpp → texto** funciona en Windows y medir cuánto tarda cada modelo. Con esos números se elige el modelo por defecto de la app.

El código está en dos crates:

- `crates/wister-core`: captura de audio (`audio.rs`), catálogo y descarga de modelos (`models.rs`) y transcripción (`stt.rs`). La app Tauri de la Fase 1 va a reutilizar este crate.
- `crates/wister-cli`: el binario `wister`, que sirve para probar y medir.

## Conseguir el binario

### Opción A: bajarlo de CI

Cada push corre el workflow **CI**, y el job de Windows publica un artifact `wister-windows-x64-cpu` con `wister.exe`. Está compilado para CPU con AVX2, que tienen prácticamente todas las PCs desde 2013/2015.

### Opción B: compilarlo

Requisitos en Windows:

1. [Rust](https://rustup.rs) (toolchain `stable-x86_64-pc-windows-msvc`).
2. **Visual Studio Build Tools** con la carga de trabajo *Desarrollo para el escritorio con C++*.
3. **CMake**: `winget install Kitware.CMake`
4. **LLVM** (lo usa `bindgen` para generar los bindings de whisper.cpp): `winget install LLVM.LLVM`

```powershell
$env:CMAKE_C_FLAGS_RELEASE   = "/MD /O2 /Ob2 /DNDEBUG"
$env:CMAKE_CXX_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"
cargo build --release -p wister-cli
.\target\release\wister.exe --help
```

Compilá siempre con `--release`. En modo debug whisper.cpp es varias veces más lento y las mediciones no sirven.

**Las dos variables `CMAKE_*_FLAGS_RELEASE` son obligatorias con MSVC.** El crate `cmake` reemplaza los flags de Release por los de `cc`, que no traen `/O2`, y whisper.cpp queda compilado sin optimizar: en un Core Ultra 7 265K, `base` tardaba 4 s en vez de 0,5 s. whisper-rs-sys le pasa a CMake cualquier variable `CMAKE_*`, y así se respeta el valor. Si ya compilaste sin ellas, corré `cargo clean --release -p whisper-rs-sys` antes, porque el build script no se vuelve a ejecutar solo.

**Rutas largas.** La build con Vulkan compila un subproyecto (`vulkan-shaders-gen`) muy anidado dentro de `target\`, y si la ruta pasa los 260 caracteres MSBuild falla con `FTK1011` / `MSB8066`. Si el repo está en una carpeta con una ruta larga, usá un `target` corto: `$env:CARGO_TARGET_DIR = "C:\wt"`.

#### Con GPU

| GPU | Requisito extra | Comando |
|---|---|---|
| Cualquiera (NVIDIA, AMD, Intel) | [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) (define `VULKAN_SDK`) | `cargo build --release -p wister-cli --features vulkan` |
| NVIDIA | [CUDA Toolkit](https://developer.nvidia.com/cuda-downloads) (define `CUDA_PATH`) | `cargo build --release -p wister-cli --features cuda` |

Un binario con GPU la usa por defecto. Con `--cpu` se fuerza la CPU, para comparar.

## Uso

```powershell
wister devices                         # micrófonos disponibles
wister models                          # catálogo y qué está descargado
wister download small                  # baja y verifica el modelo (SHA-1)

wister dictate                         # Enter para grabar, Enter para terminar
wister dictate --repeat                # varias veces seguidas con el modelo ya cargado
wister dictate -m large-v3-turbo-q5_0 --prompt "Wister, Tauri, whisper.cpp"
wister dictate -d "Blue Yeti" -s 5 --save prueba.wav

wister transcribe prueba.wav -m base
```

Los modelos se guardan en `%LOCALAPPDATA%\Wister\data\models`. Se puede usar otra carpeta con la variable `WISTER_MODELS_DIR`, o pasar la ruta a un `.bin` directamente con `-m`.

## Cómo medir

1. Bajá los modelos a comparar:
   ```powershell
   wister download base
   wister download small
   wister download large-v3-turbo-q5_0
   ```
2. Grabá un dictado de unos 10 s, hablando normal:
   ```powershell
   wister dictate -s 10 --save dictado.wav -m base
   ```
3. Corré el benchmark con ese mismo audio en todos los modelos descargados:
   ```powershell
   wister bench --file dictado.wav
   ```
   Hace una pasada de calentamiento y después 3 mediciones por modelo (se cambia con `--runs`). Imprime una tabla en Markdown y el texto que produjo cada modelo, para comparar la calidad además de la velocidad.

**RTF** (*real-time factor*) es el tiempo de proceso dividido por la duración del audio: 0,1 significa que 10 s de audio se transcriben en 1 s. Para que el dictado se sienta instantáneo, hace falta un RTF de 0,1 o menos en dictados cortos.

## Resultados

### 2026-09-27 · Intel Core Ultra 7 265K + NVIDIA RTX 5070 Ti (Vulkan), 32 GB, Windows 11

Dictado de 10 s en español. Builds locales con `/O2`, CPU con `GGML_NATIVE=OFF` + AVX2.

| Equipo | Modelo | Carga | Promedio | Mínimo | RTF |
|---|---|---:|---:|---:|---:|
| CPU, 8 hilos | base | 132 ms | 519 ms | 496 ms | 0,05 |
| CPU, 8 hilos | small | 384 ms | 1590 ms | 1557 ms | 0,16 |
| CPU, 8 hilos | large-v3-turbo-q5_0 | 439 ms | 6278 ms | 6149 ms | 0,63 |
| CPU, 16 hilos | base | 137 ms | 499 ms | 459 ms | 0,05 |
| CPU, 16 hilos | small | 361 ms | 1169 ms | 1115 ms | 0,12 |
| CPU, 16 hilos | large-v3-turbo-q5_0 | 431 ms | 4349 ms | 4307 ms | 0,44 |
| GPU (Vulkan) | base | 349 ms | 48 ms | 47 ms | 0,00 |
| GPU (Vulkan) | small | 404 ms | 83 ms | 81 ms | 0,01 |
| GPU (Vulkan) | large-v3-turbo-q5_0 | 468 ms | 79 ms | 78 ms | 0,01 |

Texto obtenido (igual en CPU y GPU):

- **base**: "Hola, ¿qué tal? Esto es una prueba de grabación para ver si funciona correctamente este sistema y reconoce *amigos en el video*." (se equivocó)
- **small**: "Hola que tal, esto es una prueba de grabación para ver si funciona correctamente este sistema y reconoce mi voz en este video."
- **large-v3-turbo-q5_0**: "Hola que tal esto es una prueba de grabación para ver si funciona correctamente este sistema y reconoce mi voz" (cortó el final, "en este video")

Conclusiones:

- Con GPU, `large-v3-turbo-q5_0` es el default obvio: tarda menos de 100 ms.
- En CPU, `small` da buena calidad con 1,2 a 1,6 s de espera; `turbo` es demasiado lento para dictar.
- Sin `/O2` (ver "Opción B") todo era unas 8 veces más lento en CPU (`base` 4032 ms, `small` 14224 ms, `turbo` 84474 ms) y un 40 a 60 % más lento en GPU.
- Pasar de 8 a 16 hilos mejora un 25 a 30 % en `small` y `turbo` (en la build sin `/O2`, 20 hilos fue peor que 16). El tope de 8 de `default_threads` es conservador para CPUs con núcleos P/E; vale la pena revisarlo en la Fase 1.

### Audio sin voz: Whisper inventa texto

| Audio | base | small | large-v3-turbo-q5_0 |
|---|---|---|---|
| 5 s de silencio digital | "y el día de hoy." | "y que no se ha hecho el video." | "Gracias." |
| 5 s de ambiente real (RMS 0,001) | "y" | "¡Suscríbete!" | "Gracias." |

Ningún modelo devuelve texto vacío. En la app, un atajo apretado sin querer pegaría basura en la ventana activa, así que **el VAD se adelanta a la Fase 1**. Mientras tanto, el nivel RMS sirve como filtro rápido: el ambiente dio 0,001 y el dictado 0,029.

## Qué falta verificar

- [x] Que compile y corra en Windows (CI + una PC real).
- [x] Latencia de `base`, `small` y `large-v3-turbo-q5_0` en CPU.
- [x] Latencia con Vulkan en la GPU que tengas.
- [ ] Calidad en español con y sin `--prompt`.
- [ ] Por qué `large-v3-turbo-q5_0` cortó el final del dictado.
- [x] Qué pasa con un audio en silencio: sí, Whisper inventa texto con los tres modelos. El VAD pasa a la Fase 1.
