# Cambios

Los cambios de cada versión. El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y las versiones, [Semantic Versioning](https://semver.org/lang/es/).

## [Sin publicar]

### Agregado

- Detección de voz con Silero VAD (incluido en la app, sin descarga): se transcriben solo los tramos con voz, se recortan los silencios y, si solo hubo ruido (tos, teclado, golpes), no se transcribe nada.
- Filtro de las frases que Whisper inventa con audio casi vacío ("Gracias.", "¡Suscríbete!", "Thank you.") cuando son todo el texto.
- CLI: opción `--vad` en `dictate` y `transcribe`.
- Historial de dictados guardado en la PC (SQLite, en `%APPDATA%\ar.wister.app\historial.db`): texto, fecha, duración del audio, cantidad de palabras y título de la ventana donde se pegó. Reemplaza a la lista de "Últimos dictados", que se perdía al cerrar la app.
- Sección Historial con buscador (sin distinguir mayúsculas ni tildes, también por la app), agrupada por día, con botones para copiar y borrar cada dictado.
- Configuración: casilla "Guardar el historial de dictados en esta PC" (activada por defecto) y botón "Borrar todo". Lo borrado se sobrescribe en el archivo.
- Sección Inicio con estadísticas de uso: palabras de hoy, de los últimos 7 días y en total, tiempo ahorrado contra tipear a 40 palabras por minuto, días seguidos dictando y velocidad; y los últimos cinco dictados.
- Sección Diccionario (por ahora, un lugar reservado para el diccionario personal).
- Doble clic en el ícono de la bandeja para abrir la ventana.
- Botón "Salir de Wister" en la barra lateral, con confirmación.

### Cambiado

- Ventana nueva, más grande (960×640), con barra lateral: Inicio, Historial, Diccionario y Configuración, y el estado del dictado abajo.
- La configuración se ordena en bloques: modelo de voz, dictado e historial.
- La ventana ya no muestra los tiempos de cada dictado; solo avisa si el último se descartó o no se pudo pegar.
- El menú de la bandeja se abre con el clic derecho, y "Configuración" pasó a llamarse "Abrir Wister".

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
