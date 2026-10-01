# Cambios

Los cambios de cada versión. El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y las versiones, [Semantic Versioning](https://semver.org/lang/es/).

## [Sin publicar]

## [0.4.0] - 2026-10-01

Un solo instalador para todas las PCs, con o sin placa de video, y varias comodidades para dictar.

### Agregado

- "Copiar el último dictado" en el menú de la bandeja: lo deja en el portapapeles y lo avisa en la pastilla de abajo.
- Configuración: "No poner punto al final", para que los mensajes de chat no terminen con punto.
- Diccionario: botón "Agregar comandos de voz", con "punto y aparte", "nueva línea", "punto y coma", "dos puntos", "abrir paréntesis" y "cerrar paréntesis". Los signos se pegan a las palabras y se sacan los puntos que Whisper agrega alrededor de cada comando: "Abrir paréntesis. Hola. Cerrar paréntesis." queda "(Hola).".
- Configuración: **Usar la placa de video**, para transcribir con el procesador aunque haya GPU. Si Wister se cierra de golpe mientras carga Vulkan o un modelo en la GPU (un driver con problemas), la vez siguiente arranca con el procesador y lo avisa.
- Aviso al elegir un modelo que no entra cómodo en la RAM de la PC (usa más de un cuarto), en Configuración y en el asistente de primer uso. El modelo recomendado también tiene en cuenta la RAM.
- CLI: `bench` con `--pausa` (esperar entre mediciones) y `--despertar`.

### Cambiado

- Si Whisper estuvo quieto unos segundos, Wister despierta la placa de video mientras hablás: después de estar quieta tardaba 2 a 3 veces más en transcribir. Solo se hace si es barato (en placas lentas demoraría el dictado).
- Los reemplazos no distinguen tildes ("nueva linea" también funciona); la ñ sí se distingue.
- **Un solo instalador**: el de siempre ahora también abre en PCs sin drivers de video o en máquinas virtuales, y ahí usa el procesador. Ya no hay instalador "solo CPU"; las instalaciones que lo tenían se actualizan solas a este.
- El instalador se muestra en español si Windows está en español (en inglés si no).
- El registro anota los segundos de voz y cuánto hacía que no se usaba Whisper en cada dictado.

## [0.3.0] - 2026-09-28

Diccionario personal, más control del micrófono y herramientas para diagnosticar problemas en otras PCs.

### Agregado

- Registro de funcionamiento en `%LOCALAPPDATA%\ar.wister.app\logs\wister.log`: datos de la PC al arrancar, carga de cada modelo (backend y memoria), tiempos de cada dictado, micrófono usado y errores. Nunca guarda el texto dictado. Al pasar de 2 MB se empieza uno nuevo y se conserva el anterior.
- Configuración → Diagnóstico → **Prueba de rendimiento**: se lee una frase y Wister la transcribe con cada modelo descargado, con GPU y con CPU; muestra la espera, el porcentaje de palabras acertadas y la carga de cada uno, y recomienda el que mejor anda en esa PC.
- Configuración → Diagnóstico → **Exportar diagnóstico**: guarda en Descargas un archivo de texto con los datos de la PC (Windows, CPU, RAM, placas de video y la GPU que usa Whisper), la configuración, los modelos, los micrófonos y el registro, para pedir ayuda desde otra máquina.
- Elegir el micrófono desde la bandeja: clic derecho → Micrófono, con "Predeterminado de Windows" y los que estén conectados (la lista se actualiza sola al enchufar uno).
- Configuración: volumen del micrófono (el mismo de Windows), aviso con un botón para activarlo si está silenciado, y un medidor de nivel con "Probar".
- Sección **Diccionario**:
  - **Vocabulario**: palabras que Whisper no conoce (nombres propios, marcas, jerga) y que se le pasan como pista en cada dictado para que las escriba así.
  - **Reemplazos**: cambios sobre el texto transcripto, que se aplican siempre ("Chat GPT" → "ChatGPT"). Buscan palabras o frases completas sin distinguir mayúsculas; con el segundo campo vacío borran la frase, y `\n` es un salto de línea (para "punto y aparte").
- "Acerca de Wister" en Configuración, con quién lo hace, la licencia y enlaces al código, a los problemas reportados y a la licencia.

### Cambiado

- La nota de `large-v3-turbo` ahora dice que es el más pesado y que conviene solo con placas de video potentes.
- Con Vulkan, el modelo recomendado depende de si la PC tiene una GPU compatible, y no solo de cómo se compiló la app. Sin GPU, se recomienda `small` y el estado dice que usa la CPU.

### Corregido

- El medidor de nivel de Configuración se apaga bien si se empieza a dictar o se arranca la prueba de rendimiento (antes quedaba congelado).

## [0.2.0] - 2026-09-27

Primera versión con instaladores para descargar.

### Agregado

- Detección de voz con Silero VAD (incluido en la app, sin descarga): se transcriben solo los tramos con voz, se recortan los silencios y, si solo hubo ruido (tos, teclado, golpes), no se transcribe nada.
- Filtro de las frases que Whisper inventa con audio casi vacío ("Gracias.", "¡Suscríbete!", "Thank you.") cuando son todo el texto.
- CLI: opción `--vad` en `dictate` y `transcribe`.
- Historial de dictados guardado en la PC (SQLite, en `%APPDATA%\ar.wister.app\historial.db`): texto, fecha, duración del audio, cantidad de palabras y título de la ventana donde se pegó. Reemplaza a la lista de "Últimos dictados", que se perdía al cerrar la app.
- Sección Historial con buscador (sin distinguir mayúsculas ni tildes, también por la app), agrupada por día, con botones para copiar y borrar cada dictado.
- Configuración: casilla "Guardar el historial de dictados en esta PC" (activada por defecto) y botón "Borrar todo". Lo borrado se sobrescribe en el archivo.
- Sección Inicio con estadísticas de uso: palabras de hoy, de los últimos 7 días y en total, tiempo ahorrado contra tipear a 40 palabras por minuto, días seguidos dictando y velocidad; y los últimos cinco dictados.
- Doble clic en el ícono de la bandeja para abrir la ventana.
- Botón "Salir de Wister" en la barra lateral, con confirmación.
- Actualización con un botón: "Buscar actualizaciones" en Configuración baja la versión nueva desde GitHub Releases, verifica su firma y la instala. Solo se conecta cuando se toca el botón.
- Dos instaladores: con Vulkan (usa la placa de video, o el procesador si no hay) y solo CPU, para PCs sin Vulkan. Cada uno se actualiza a su misma variante.

### Cambiado

- Ventana nueva, más grande (960×640), con barra lateral: Inicio, Historial y Configuración, y el estado del dictado abajo.
- La configuración se ordena en bloques: modelo de voz, dictado, historial y "Acerca de".
- La ventana ya no muestra los tiempos de cada dictado; solo avisa si el último se descartó o no se pudo pegar.
- El menú de la bandeja se abre con el clic derecho, y "Configuración" pasó a llamarse "Abrir Wister".

### Corregido

- El instalador con Vulkan usa instrucciones de CPU portables (AVX2): antes usaba las de la PC donde se compilaba y, sin GPU, podía cerrarse en otros procesadores.

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

[Sin publicar]: https://github.com/ddepolo/Wister/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/ddepolo/Wister/releases/tag/v0.4.0
[0.3.0]: https://github.com/ddepolo/Wister/releases/tag/v0.3.0
[0.2.0]: https://github.com/ddepolo/Wister/releases/tag/v0.2.0
[0.1.0]: https://github.com/ddepolo/Wister/releases/tag/v0.1.0
