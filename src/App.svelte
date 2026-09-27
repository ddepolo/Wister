<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Asistente from "./Asistente.svelte";
  import CapturaAtajo from "./CapturaAtajo.svelte";
  import {
    IDIOMAS,
    nombreAtajo,
    type Config,
    type Descarga,
    type Estado,
    type Microfono,
    type Modelo,
  } from "./tipos";

  type Pegado =
    | { tipo: "hecho" | "ventana_elevada" | "teclas_apretadas" }
    | { tipo: "error"; mensaje: string };

  type Resultado =
    | {
        tipo: "texto";
        texto: string;
        audio_ms: number;
        espera_ms: number;
        microfono_ms: number;
        pegado: Pegado;
      }
    | { tipo: "descartado"; motivo: "otra_tecla" | "toque_corto" | "sin_voz" | "sin_texto" };

  const DESCARTES = {
    otra_tecla: "se tocó otra tecla",
    toque_corto: "toque muy corto",
    sin_voz: "no se detectó voz",
    sin_texto: "Whisper no devolvió texto",
  };

  let version = $state("");
  let estado = $state<Estado>({ tipo: "iniciando" });
  let historial = $state<{ hora: string; resultado: Resultado }[]>([]);
  let config = $state<Config | null>(null);
  let modelos = $state<Modelo[]>([]);
  let microfonos = $state<Microfono[]>([]);
  let descargas = $state<Record<string, { porcentaje: number | null; error?: string }>>({});
  let errorAjustes = $state("");
  let autoarranque = $state(false);

  const recomendado = $derived(modelos.find((m) => m.recomendado)?.nombre ?? "");
  const modeloElegido = $derived(config?.modelo ?? recomendado);
  const microfonoPredeterminado = $derived(microfonos.find((m) => m.predeterminado)?.nombre);

  getVersion().then((v) => (version = v));
  invoke<Estado>("estado_actual").then((e) => (estado = e));
  invoke<Config>("obtener_config")
    .then((c) => (config = c))
    .catch((e) => (errorAjustes = `No se pudo leer la configuración: ${e}`));
  invoke<boolean>("autoarranque").then((a) => (autoarranque = a));
  refrescar();
  // Al volver a la ventana se releen los micrófonos: puede que se haya enchufado uno.
  window.addEventListener("focus", refrescar);

  listen<Estado>("estado", ({ payload }) => (estado = payload));
  listen<Resultado>("resultado", ({ payload }) => {
    const hora = new Date().toLocaleTimeString("es-AR");
    historial = [{ hora, resultado: payload }, ...historial].slice(0, 20);
  });
  listen<Descarga>("descarga", ({ payload }) => {
    const { modelo } = payload;
    if (payload.tipo === "progreso") {
      const porcentaje = payload.total ? (payload.bajado / payload.total) * 100 : null;
      descargas[modelo] = { porcentaje };
    } else if (payload.tipo === "lista") {
      delete descargas[modelo];
      refrescar();
    } else {
      descargas[modelo] = { porcentaje: null, error: payload.mensaje };
    }
  });

  function refrescar() {
    invoke<Modelo[]>("listar_modelos").then((m) => (modelos = m));
    invoke<Microfono[]>("listar_microfonos")
      .then((m) => (microfonos = m))
      .catch((e) => (errorAjustes = String(e)));
  }

  async function cambiar(cambios: Partial<Config>) {
    if (!config) return;
    const nueva = { ...config, ...cambios };
    try {
      await invoke("guardar_config", { config: nueva });
      config = nueva;
      errorAjustes = "";
    } catch (e) {
      errorAjustes = String(e);
    }
  }

  async function cambiarAutoarranque(activo: boolean) {
    try {
      await invoke("cambiar_autoarranque", { activo });
      autoarranque = activo;
      errorAjustes = "";
    } catch (e) {
      errorAjustes = String(e);
    }
  }

  function descargar(nombre: string) {
    descargas[nombre] = { porcentaje: 0 };
    invoke("descargar_modelo", { nombre }).catch((e) => {
      descargas[nombre] = { porcentaje: null, error: String(e) };
    });
  }

  function noPegado(p: Pegado): string | null {
    switch (p.tipo) {
      case "hecho":
        return null;
      case "ventana_elevada":
        return "No se pudo pegar: la ventana corre como administrador. Quedó en el portapapeles.";
      case "teclas_apretadas":
        return "No se pegó porque seguían apretadas teclas modificadoras. Quedó en el portapapeles.";
      case "error":
        return `No se pudo pegar (${p.mensaje}).`;
    }
  }

  const etiqueta = $derived(
    {
      iniciando: "Iniciando…",
      cargando_modelo: "Cargando modelo…",
      listo: "En espera",
      grabando: "● Grabando",
      transcribiendo: "Transcribiendo…",
      error: "Error",
    }[estado.tipo],
  );

  const seg = (ms: number) => (ms / 1000).toFixed(1).replace(".", ",") + " s";
</script>

{#if config && !config.asistente_completo}
  <Asistente
    {config}
    onterminar={(c) => {
      config = c;
      refrescar();
    }}
  />
{:else}
<main>
  <h1>Wister</h1>
  <p>
    Mantené apretado <kbd>{config ? nombreAtajo(config.atajo) : "…"}</kbd>, hablá y soltá.
  </p>

  <div class="barra">
    <span class="estado {estado.tipo}">{etiqueta}</span>
    {#if estado.tipo === "listo"}
      <span class="muted">{estado.modelo} · {estado.gpu ? "GPU" : "CPU"}</span>
    {:else if estado.tipo === "cargando_modelo"}
      <span class="muted">{estado.modelo}</span>
    {/if}
  </div>
  {#if estado.tipo === "error"}
    <p class="error">{estado.mensaje}</p>
  {/if}

  {#if config}
    <section>
      <h2>Modelo</h2>
      <ul class="modelos">
        {#each modelos as m}
          {@const descarga = descargas[m.nombre]}
          <li class:elegido={m.nombre === modeloElegido}>
            <label>
              <input
                type="radio"
                name="modelo"
                value={m.nombre}
                checked={m.nombre === modeloElegido}
                disabled={!m.descargado}
                onchange={() => cambiar({ modelo: m.nombre })}
              />
              <span class="nombre">{m.nombre}</span>
              {#if m.recomendado}<span class="insignia">Recomendado</span>{/if}
              <span class="muted derecha">{m.mb} MB</span>
            </label>
            <div class="nota muted">{m.nota}</div>
            {#if descarga && !descarga.error}
              <div class="progreso">
                <div style="width: {descarga.porcentaje ?? 0}%"></div>
              </div>
            {:else if !m.descargado}
              <button onclick={() => descargar(m.nombre)}>Descargar</button>
              {#if descarga?.error}<span class="error">{descarga.error}</span>{/if}
            {/if}
          </li>
        {/each}
      </ul>

      <div class="fila">
        <label for="microfono">Micrófono</label>
        <select
          id="microfono"
          value={config.microfono ?? ""}
          onchange={(e) => cambiar({ microfono: e.currentTarget.value || null })}
        >
          <option value="">
            Predeterminado de Windows{microfonoPredeterminado ? ` (${microfonoPredeterminado})` : ""}
          </option>
          {#each microfonos as m}
            <option value={m.nombre}>{m.nombre}</option>
          {/each}
          {#if config.microfono && !microfonos.some((m) => m.nombre === config?.microfono)}
            <option value={config.microfono}>{config.microfono} (no conectado)</option>
          {/if}
        </select>
      </div>

      <div class="fila">
        <label for="idioma">Idioma</label>
        <select
          id="idioma"
          value={config.idioma}
          onchange={(e) => cambiar({ idioma: e.currentTarget.value })}
        >
          {#each IDIOMAS as [codigo, nombre]}
            <option value={codigo}>{nombre}</option>
          {/each}
        </select>
      </div>

      <div class="fila">
        <span class="etiqueta">Atajo</span>
        <CapturaAtajo atajo={config.atajo} oncambiar={(atajo) => cambiar({ atajo })} />
      </div>

      <label class="casilla">
        <input
          type="checkbox"
          checked={config.mostrar_overlay}
          onchange={(e) => cambiar({ mostrar_overlay: e.currentTarget.checked })}
        />
        Mostrar la onda mientras grabo
      </label>
      <label class="casilla">
        <input
          type="checkbox"
          checked={config.sonidos}
          onchange={(e) => cambiar({ sonidos: e.currentTarget.checked })}
        />
        Sonido al empezar y al terminar de grabar
      </label>
      <label class="casilla">
        <input
          type="checkbox"
          checked={autoarranque}
          onchange={(e) => cambiarAutoarranque(e.currentTarget.checked)}
        />
        Iniciar Wister con Windows
      </label>

      {#if errorAjustes}<p class="error">{errorAjustes}</p>{/if}
    </section>
  {:else if errorAjustes}
    <p class="error">{errorAjustes}</p>
  {/if}

  <section>
    <h2>Últimos dictados</h2>
    <ul class="historial">
      {#each historial as { hora, resultado }}
        <li>
          <span class="muted">{hora}</span>
          {#if resultado.tipo === "texto"}
            <p class="texto">{resultado.texto}</p>
            <span class="muted">
              {seg(resultado.audio_ms)} de audio · texto en {resultado.espera_ms} ms · micrófono abierto
              en {resultado.microfono_ms} ms
            </span>
            {#if noPegado(resultado.pegado)}
              <p class="aviso">{noPegado(resultado.pegado)}</p>
            {/if}
          {:else}
            <span class="descartado">Descartado: {DESCARTES[resultado.motivo]}</span>
          {/if}
        </li>
      {:else}
        <li class="muted">Todavía no dictaste nada.</li>
      {/each}
    </ul>
  </section>

  <p class="muted pie">
    Versión {version} ·
    <button class="enlace" onclick={() => cambiar({ asistente_completo: false })}>
      Volver a ejecutar el asistente
    </button>
  </p>
</main>
{/if}

<style>
  :global(body) {
    margin: 0;
    font-family: "Segoe UI", system-ui, sans-serif;
    background: #f7f7f8;
    color: #1d1d1f;
    --borde: rgba(127, 127, 127, 0.25);
    --acento: #4f46e5;
  }
  @media (prefers-color-scheme: dark) {
    :global(body) {
      background: #1e1e20;
      color: #ececf1;
      --acento: #818cf8;
    }
  }
  main {
    padding: 20px 24px;
  }
  h1 {
    margin: 0 0 8px;
    font-size: 22px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 14px;
    font-weight: 600;
  }
  section {
    margin-top: 18px;
  }
  kbd {
    padding: 1px 6px;
    border: 1px solid currentColor;
    border-radius: 4px;
    font-size: 12px;
    opacity: 0.8;
  }
  .barra {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 4px 0 4px;
  }
  .estado {
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 13px;
    background: rgba(127, 127, 127, 0.15);
  }
  .estado.grabando {
    background: #dc2626;
    color: #fff;
  }
  .estado.transcribiendo {
    background: #4f46e5;
    color: #fff;
  }
  .estado.error,
  .error {
    color: #dc2626;
    font-size: 13px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 13px;
  }
  .modelos li {
    padding: 8px 10px;
    border: 1px solid var(--borde);
    border-radius: 8px;
    margin-bottom: 6px;
  }
  .modelos li.elegido {
    border-color: var(--acento);
  }
  .modelos label {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .nombre {
    font-weight: 600;
  }
  .insignia {
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 11px;
    background: var(--acento);
    color: #fff;
  }
  .derecha {
    margin-left: auto;
  }
  .nota {
    margin: 2px 0 0 24px;
  }
  .modelos button {
    margin: 6px 0 0 24px;
  }
  button,
  select {
    font: inherit;
    font-size: 13px;
    color: inherit;
    background: rgba(127, 127, 127, 0.12);
    border: 1px solid var(--borde);
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
  }
  select option {
    color: #1d1d1f;
  }
  .progreso {
    margin: 8px 0 2px 24px;
    height: 6px;
    border-radius: 3px;
    background: rgba(127, 127, 127, 0.2);
    overflow: hidden;
  }
  .progreso div {
    height: 100%;
    background: var(--acento);
    transition: width 150ms linear;
  }
  .fila {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
    font-size: 13px;
  }
  .fila label,
  .fila .etiqueta {
    width: 80px;
    flex: none;
  }
  .fila select {
    flex: 1;
    min-width: 0;
  }
  .casilla {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: 13px;
  }
  .historial li {
    padding: 8px 0;
    border-bottom: 1px solid var(--borde);
  }
  .texto {
    margin: 2px 0;
    font-size: 15px;
  }
  .aviso {
    margin: 4px 0 0;
    color: #d97706;
  }
  .descartado {
    opacity: 0.75;
  }
  .muted {
    opacity: 0.6;
    font-size: 12px;
  }
  .pie {
    margin-top: 12px;
  }
  .enlace {
    padding: 0;
    border: none;
    background: none;
    font-size: 12px;
    color: var(--acento);
    text-decoration: underline;
  }
</style>
