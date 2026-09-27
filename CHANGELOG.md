# Cambios

Los cambios de cada versión. El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y las versiones, [Semantic Versioning](https://semver.org/lang/es/).

## [0.1.0] - 2026-09-27

Primera versión pública.

### Agregado

- App de bandeja para Windows con dictado push-to-talk: mantener el atajo, hablar y soltar pega el texto en la aplicación activa.
- Atajo global con Raw Input: `Ctrl` + `Shift` izquierdos por defecto, configurable (`Ctrl`, `Shift`, `Alt` de cada lado y F1–F24). Tocar otra tecla mientras está apretado cancela, y los toques de menos de 300 ms se ignoran.
- Transcripción local con whisper.cpp, en GPU (Vulkan) o CPU. Modelos `base`, `small`, `large-v3-turbo-q5_0` y `large-v3-turbo`, descargados desde la app y verificados con SHA-1.
- Pegado por portapapeles que restaura el contenido anterior (texto, imágenes y otros formatos) y excluye el dictado del historial de `Win+V`. Si la ventana activa corre como administrador, el texto queda en el portapapeles y se avisa.
- Espacio automático entre dictados seguidos en la misma ventana.
- Overlay con la forma de onda mientras se graba, que no le saca el foco a la aplicación activa.
- Filtro de grabaciones sin voz, para que Whisper no invente texto a partir del silencio.
- Asistente de primer uso: idioma, modelo, micrófono con medidor de nivel y dictado de prueba.
- Configuración: modelo, micrófono, idioma, atajo, overlay, sonidos al grabar y arranque con Windows.
- CLI `wister` para probar y medir modelos (`devices`, `models`, `download`, `dictate`, `transcribe`, `bench`).
- Instalador NSIS por usuario, sin permisos de administrador.

[0.1.0]: https://github.com/ddepolo/Wister/releases/tag/v0.1.0
