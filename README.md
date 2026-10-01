# Wister

[![CI](https://github.com/ddepolo/Wister/actions/workflows/ci.yml/badge.svg)](https://github.com/ddepolo/Wister/actions/workflows/ci.yml)
[![Licencia: GPL v3](https://img.shields.io/badge/licencia-GPL--3.0--or--later-blue.svg)](LICENSE)

*[English summary](README.en.md)*

**Dictado por voz local, libre y gratuito para Windows.** Mantenés apretado un atajo, hablás, soltás, y el texto aparece donde tengas el cursor: en el navegador, el editor, el chat o la terminal.

La transcripción la hace [Whisper](https://github.com/openai/whisper) (vía [whisper.cpp](https://github.com/ggml-org/whisper.cpp)) **en tu PC**: el audio no sale de tu computadora, no hay cuentas ni suscripción, y no hay telemetría. La red se usa solamente para bajar el modelo de voz, cuando lo pedís, y para ver si hay una versión nueva (se puede apagar).

> **Estado: 0.4.0.** Funciona y se usa a diario, pero es una versión temprana: todavía puede cambiar bastante. Ver el [roadmap](#roadmap).

## Qué hace

- **Push-to-talk global**: `Ctrl` + `Shift` izquierdos por defecto, configurable (`Ctrl`, `Shift` o `Alt` de cualquier lado, y teclas F).
- **Pega en cualquier aplicación** y después restaura lo que tenías en el portapapeles, incluso imágenes. El texto dictado no queda en el historial de `Win+V`.
- **Rápido**: con GPU, el texto aparece unos 250 ms después de soltar el atajo.
- **Funciona con o sin GPU**: aprovecha la placa de video con Vulkan (NVIDIA, AMD o Intel), y si no hay, usa el procesador. Un solo instalador para todas las PCs, y se puede elegir el procesador a mano.
- **Onda en pantalla** mientras grabás, en una pastilla chiquita que nunca le saca el foco a la app donde estás escribiendo.
- **Asistente de primer uso**: idioma, descarga del modelo, prueba del micrófono y un primer dictado de prueba.
- **Estadísticas de uso**: palabras dictadas hoy, en la semana y en total, tiempo ahorrado contra tipear, días seguidos y velocidad.
- **Historial en tu PC**: cada dictado queda guardado con la fecha y la app donde lo pegaste, con buscador y botones para copiarlo o borrarlo. Se puede desactivar y borrar todo.
- **Configurable**: modelo, micrófono (también desde la bandeja), su volumen, idioma, atajo, sonidos al grabar y arranque con Windows.
- **Diccionario personal**: palabras que Whisper tiene que conocer (nombres, marcas, jerga) y reemplazos que se aplican siempre, como "punto y aparte" → salto de línea o sacar muletillas.
- **Diagnóstico**: una prueba de rendimiento te dice qué modelo anda mejor en tu PC, y "Exportar diagnóstico" arma un archivo de texto para pedir ayuda (sin nada de lo que dictaste).
- **Se actualiza con un botón**: Wister te avisa en Inicio cuando hay una versión nueva, y "Actualizar ahora" la baja, verifica su firma y la instala. El aviso se puede apagar.
- **No inventa texto**: un detector de voz (Silero VAD) deja pasar solo lo que dijiste y recorta los silencios. Si solo hubo ruido, no transcribe nada (Whisper suele "escuchar" un "Gracias." en grabaciones vacías).

## Requisitos

- Windows 10 u 11 de 64 bits.
- Un procesador con AVX2 (prácticamente cualquiera desde 2015).
- Recomendado: una placa de video compatible con Vulkan. Sin GPU funciona igual, con un modelo más chico y algo más lento.
- Entre 0,5 y 1,5 GB de disco para el modelo de voz.

## Instalación

Bajá `Wister_<versión>_x64-setup.exe` de la [última versión](https://github.com/ddepolo/Wister/releases/latest). Usa la placa de video con Vulkan (NVIDIA, AMD o Intel) y, si no hay o no tiene drivers, el procesador. Si preferís el procesador, apagá **Usar la placa de video** en Configuración.

Se instala solo para tu usuario, sin permisos de administrador. El modelo de voz se baja después, desde el asistente de primer uso.

Como el instalador todavía no está firmado, Windows SmartScreen va a avisar que es de un editor desconocido: **Más información → Ejecutar de todas formas**. Si querés verificar lo que bajaste, cada versión trae un `SHA256SUMS.txt` (`Get-FileHash .\Wister_...exe` en PowerShell).

Para actualizar: **Configuración → Buscar actualizaciones**. Si tenías el instalador "solo CPU" de las versiones anteriores, se actualiza solo a este.

## Uso

1. La primera vez se abre un asistente: elegís el idioma, se descarga el modelo recomendado para tu PC y probás el micrófono.
2. Wister queda en la **bandeja del sistema**, al lado del reloj.
3. Poné el cursor donde quieras escribir, **mantené apretado el atajo**, hablá y **soltá**.

Si mientras tenés el atajo apretado tocás otra tecla, se cancela: así los atajos comunes como `Ctrl+Shift+T` siguen funcionando. Un toque de menos de 300 ms también se ignora.

Con doble clic en el ícono de la bandeja (o clic derecho → "Abrir Wister") se abre la ventana: en **Inicio** están tus estadísticas (palabras dictadas, tiempo ahorrado, días seguidos) y los últimos dictados; en **Historial**, todo lo que dictaste, con buscador; y en **Configuración**, el modelo, el micrófono y su volumen, el idioma, el atajo, la onda en pantalla, los sonidos, el arranque con Windows y el diagnóstico. El micrófono también se elige con clic derecho en el ícono de la bandeja → Micrófono.

Si no sabés qué modelo usar, **Configuración → Diagnóstico → Prueba de rendimiento**: leés una frase y Wister la transcribe con cada modelo que tenés descargado, te muestra cuánto tarda y cuánto acierta cada uno, y te recomienda uno.

## Modelos

Wister usa los modelos multilingües de Whisper en el formato de whisper.cpp. Se bajan de [Hugging Face](https://huggingface.co/ggerganov/whisper.cpp) y se verifican con el hash SHA-1 que publica whisper.cpp.

| Modelo | Tamaño | Para qué | 10 s de audio, GPU | 10 s de audio, CPU |
|---|---:|---|---:|---:|
| `base` | 142 MB | PCs viejas; rápido pero se equivoca más | ~50 ms | ~0,5 s |
| `small` | 466 MB | **Recomendado sin GPU** | ~80 ms | ~1,2–1,6 s |
| `large-v3-turbo-q5_0` | 547 MB | **Recomendado con GPU**: la mejor calidad por MB | ~80 ms | ~4–6 s |
| `large-v3-turbo` | 1,5 GB | Sin cuantizar; casi no mejora a q5_0 | — | — |

Los tiempos se midieron con `wister bench` en un Intel Core Ultra 7 265K con una RTX 5070 Ti (Vulkan). En tu PC van a variar. El detalle está en [`docs/fase-0.md`](docs/fase-0.md).

Los modelos se guardan en `%LOCALAPPDATA%\Wister\data\models`.

## Privacidad

- El audio se procesa en memoria y no se guarda en disco.
- No hay telemetría, analíticas ni reportes automáticos de errores.
- Se conecta a internet solo para bajar modelos, cuando lo pedís, y para ver si hay una versión nueva: al abrir Wister y una vez por día le pide a GitHub el archivo `latest.json`, sin mandar ningún dato tuyo. Esto último se apaga en Configuración → Acerca de.
- La configuración se guarda en `%APPDATA%\ar.wister.app\config.json`.
- El historial de dictados (el texto, no el audio) se guarda en `%APPDATA%\ar.wister.app\historial.db`, y solo si la opción está activada. "Borrar todo" lo elimina del archivo.
- Un registro de funcionamiento (tiempos, errores y datos de la PC, nunca el texto dictado) se guarda en `%LOCALAPPDATA%\ar.wister.app\logs\wister.log`. Solo sale de tu PC si exportás el diagnóstico y lo compartís.

## Compilar desde el código

Requisitos en Windows:

1. [Rust](https://rustup.rs) con el toolchain `stable-x86_64-pc-windows-msvc`.
2. Visual Studio Build Tools con *Desarrollo para el escritorio con C++*.
3. [CMake](https://cmake.org) y [LLVM](https://llvm.org) (`winget install Kitware.CMake LLVM.LLVM`).
4. [Node.js](https://nodejs.org) 24 o más nuevo.
5. Para la GPU: el [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) (`winget install KhronosGroup.VulkanSDK`).

```powershell
npm install
.\scripts\dev.ps1          # corre la app en modo desarrollo (con Vulkan; -Cpu para solo CPU)
.\scripts\build.ps1        # genera el instalador (con Vulkan; -Cpu para solo CPU)
.\scripts\release.ps1      # los dos instaladores + latest.json para publicar una versión
```

Los scripts arman el entorno que necesita whisper.cpp en Windows. La primera compilación tarda varios minutos. Más detalles, y la CLI de pruebas y benchmarks, en [`docs/fase-0.md`](docs/fase-0.md).

## Cómo está hecho

| Pieza | Tecnología |
|---|---|
| App | [Tauri 2](https://tauri.app) (Rust + WebView2) |
| Interfaz | [Svelte 5](https://svelte.dev) + TypeScript, con Vite |
| Transcripción | [whisper.cpp](https://github.com/ggml-org/whisper.cpp) vía [`whisper-rs`](https://github.com/tazz4843/whisper-rs), en CPU o GPU (Vulkan; CUDA como opción de compilación) |
| Audio | [`cpal`](https://github.com/RustAudio/cpal) (WASAPI) y [`rubato`](https://github.com/HEnquist/rubato) para remuestrear a 16 kHz |
| Atajo | Raw Input de Windows |
| Pegado | Portapapeles + `Ctrl+V` con `SendInput`, restaurando el contenido anterior |

El diseño, las decisiones y sus porqués están en [`docs/arquitectura.md`](docs/arquitectura.md).

```
crates/wister-core/   captura de audio, modelos y transcripción (biblioteca)
crates/wister-cli/    CLI para probar y medir: wister dictate, wister bench...
src-tauri/            la app: bandeja, atajo, dictado, pegado, overlay, configuración
src/                  la interfaz (inicio, historial, configuración, asistente, overlay)
scripts/              dev.ps1, build.ps1 y el generador del ícono
docs/                 arquitectura y mediciones
```

## Roadmap

**Próximo**
- Instaladores compilados en GitHub y firmados, para que Windows no avise que el editor es desconocido.
- Tema claro, oscuro o automático, y el idioma desde la bandeja.

**Más adelante**
- Modo manos libres (tocar para empezar y para terminar).
- Post-procesado opcional con un modelo de lenguaje local, para sacar muletillas y ajustar el tono.
- macOS y Linux.

## Contribuir

¡Las contribuciones son bienvenidas! Leé [`CONTRIBUTING.md`](CONTRIBUTING.md) para preparar el entorno y conocer las convenciones del proyecto.

## Licencia

[GPL-3.0-or-later](LICENSE): podés usar, estudiar, modificar y compartir Wister, y cualquier versión derivada tiene que seguir siendo libre.

Wister usa [whisper.cpp](https://github.com/ggml-org/whisper.cpp) y los modelos [Whisper](https://github.com/openai/whisper) de OpenAI, ambos con licencia MIT; [Silero VAD](https://github.com/snakers4/silero-vad) (MIT), cuyo modelo viene incluido en `crates/wister-core/assets/`, y [Tauri](https://tauri.app) (MIT/Apache-2.0).
