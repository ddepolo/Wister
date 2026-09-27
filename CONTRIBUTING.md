# Cómo contribuir

¡Gracias por querer sumarte! Wister es un proyecto chico y cualquier ayuda cuenta: reportar un error, probarlo en otro hardware, mejorar la documentación o escribir código.

*Contributions in English are welcome too: open issues and pull requests in whichever language you prefer.*

## Principios del proyecto

Antes de proponer una función, tené en cuenta lo que Wister no negocia:

- **Todo local**: el audio y el texto no salen de la PC.
- **Red solo a pedido**: la app no se conecta a internet salvo cuando el usuario pide algo explícitamente (hoy, bajar un modelo).
- **Sin telemetría**, sin cuentas y sin suscripciones.
- **Liviano**: nada de runtimes pesados ni servicios en segundo plano innecesarios.

## Reportar un problema

Abrí un [issue](https://github.com/ddepolo/Wister/issues) con:

- Qué hiciste, qué esperabas y qué pasó.
- Versión de Windows, procesador y placa de video.
- El modelo y el idioma que tenías elegidos.
- Si el problema es con una aplicación en particular (por ejemplo, "no pega en tal programa"), cuál.

## Preparar el entorno

Seguí la sección [Compilar desde el código](README.md#compilar-desde-el-código) del README. Resumen:

```powershell
npm install
.\scripts\dev.ps1          # corre la app en modo desarrollo
```

`scripts/dev.ps1` arma lo que necesita whisper.cpp en Windows: los flags de optimización (sin ellos queda sin `/O2` y es unas 8 veces más lento) y una carpeta de compilación de ruta corta (`C:\wt`), porque Vulkan supera el límite de 260 caracteres de MSBuild. Los porqués están en [`docs/arquitectura.md`](docs/arquitectura.md).

Para probar la transcripción sin la app está la CLI (`crates/wister-cli`), documentada en [`docs/fase-0.md`](docs/fase-0.md).

## Antes de mandar un pull request

```powershell
cargo fmt
cargo clippy --all-targets -- -D warnings   # CI falla con cualquier warning
cargo test
npm run check                               # svelte-check
```

CI corre lo mismo en Linux y Windows.

Si tu cambio toca algo que no se puede testear automáticamente (el atajo, el pegado, el overlay), contá en el PR cómo lo probaste y en qué aplicaciones.

## Convenciones

- **Idioma**: la documentación, los comentarios, los mensajes de error y la interfaz van en español (rioplatense, con voseo: "tenés", "elegí"). Los **mensajes de commit van en inglés**.
- **Errores**: `anyhow` con `.context("…")` en español. Los mensajes para el usuario dicen qué hacer, por ejemplo "descargalo en Configuración".
- **Tests**: en el mismo archivo (`#[cfg(test)] mod tests`), con nombres en español que describen el comportamiento (`un_toque_corto_se_descarta`).
- **Comentarios**: solo donde explican un *por qué* que no es obvio. El *qué* lo tiene que contar el código.
- **Commits chicos**, uno por cambio lógico, con un mensaje que explique el motivo además del cambio.
- Si usás un asistente de IA, está bien: [`CLAUDE.md`](CLAUDE.md) tiene el contexto del proyecto para Claude Code. Revisá y probá lo que genere como si lo hubieras escrito vos.

## Licencia de las contribuciones

Al contribuir aceptás que tu aporte se publique bajo la misma licencia del proyecto, [GPL-3.0-or-later](LICENSE).
