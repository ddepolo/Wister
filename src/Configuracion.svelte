<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, untrack } from "svelte";
  import CapturaAtajo from "./CapturaAtajo.svelte";
  import Rendimiento from "./Rendimiento.svelte";
  import {
    IDIOMAS,
    type Config,
    type Descarga,
    type EstadoGpu,
    type Microfono,
    type Modelo,
    type Volumen,
  } from "./tipos";

  let {
    config,
    cambiar,
    visible,
  }: {
    config: Config;
    cambiar: (cambios: Partial<Config>) => Promise<void>;
    /** Si la sección se ve: el medidor se apaga al salir, para no dejar el micrófono abierto. */
    visible: boolean;
  } = $props();

  let version = $state("");
  let modelos = $state<Modelo[]>([]);
  let gpu = $state<EstadoGpu | null>(null);
  let microfonos = $state<Microfono[]>([]);
  let descargas = $state<Record<string, { porcentaje: number | null; error?: string }>>({});
  let autoarranque = $state(false);
  let confirmarBorrado = $state(false);
  let diagnostico = $state<{ tipo: "inactivo" | "exportando" } | { tipo: "listo"; ruta: string }>({
    tipo: "inactivo",
  });
  let error = $state("");
  let volumen = $state<Volumen | null>(null);
  let errorVolumen = $state("");
  let probando = $state(false);
  let nivel = $state(0);

  type Actualizacion =
    | { tipo: "inactiva" | "buscando" | "al_dia" }
    | { tipo: "disponible"; version: string; notas: string | null }
    | { tipo: "instalando"; version: string; porcentaje: number | null }
    | { tipo: "error"; mensaje: string };
  let actualizacion = $state<Actualizacion>({ tipo: "inactiva" });

  const recomendado = $derived(modelos.find((m) => m.recomendado)?.nombre ?? "");
  const modeloElegido = $derived(config.modelo ?? recomendado);
  const microfonoPredeterminado = $derived(microfonos.find((m) => m.predeterminado)?.nombre);

  onMount(() => {
    getVersion().then((v) => (version = v));
    invoke<boolean>("autoarranque").then((a) => (autoarranque = a));
    invoke<EstadoGpu>("estado_gpu").then((g) => (gpu = g));
    refrescar();
    // Al volver a la ventana se releen los micrófonos: puede que se haya enchufado uno.
    window.addEventListener("focus", refrescar);
    window.addEventListener("blur", dejarDeProbar);
    const medidor = listen<number>("nivel_prueba", ({ payload }) => {
      if (probando) nivel = Math.min(1, Math.sqrt(payload / 0.12));
    });
    // El dictado y la prueba de rendimiento tienen prioridad y cierran el micrófono.
    const fin = listen("prueba_detenida", () => {
      probando = false;
      nivel = 0;
    });
    const progreso = listen<{ bajado: number; total: number | null }>(
      "actualizacion",
      ({ payload }) => {
        if (actualizacion.tipo === "instalando") {
          actualizacion.porcentaje = payload.total ? (payload.bajado / payload.total) * 100 : null;
        }
      },
    );
    const escucha = listen<Descarga>("descarga", ({ payload }) => {
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
    return () => {
      window.removeEventListener("focus", refrescar);
      window.removeEventListener("blur", dejarDeProbar);
      medidor.then((dejar) => dejar());
      fin.then((dejar) => dejar());
      dejarDeProbar();
      escucha.then((dejar) => dejar());
      progreso.then((dejar) => dejar());
    };
  });

  function refrescar() {
    invoke<Modelo[]>("listar_modelos").then((m) => (modelos = m));
    invoke<Microfono[]>("listar_microfonos")
      .then((m) => (microfonos = m))
      .catch((e) => (error = String(e)));
    // El volumen también se puede haber cambiado desde Windows.
    leerVolumen(config.microfono);
  }

  function leerVolumen(nombre: string | null) {
    invoke<Volumen>("obtener_volumen", { nombre })
      .then((v) => {
        volumen = v;
        errorVolumen = "";
      })
      .catch((e) => {
        volumen = null;
        errorVolumen = String(e);
      });
  }

  // Al cambiar de micrófono (acá o desde la bandeja), su volumen y, si se está
  // probando, el medidor.
  $effect(() => {
    const nombre = config.microfono;
    leerVolumen(nombre);
    if (untrack(() => probando)) invoke("probar_microfono", { nombre });
  });

  $effect(() => {
    if (!visible) dejarDeProbar();
  });

  function probar() {
    probando = true;
    nivel = 0;
    invoke("probar_microfono", { nombre: config.microfono });
  }

  function dejarDeProbar() {
    if (!probando) return;
    probando = false;
    nivel = 0;
    invoke("detener_prueba_microfono");
  }

  // Mientras se arrastra el control llegan muchos cambios: se manda uno por vez y,
  // al terminar, el último que quedó pendiente.
  let enviando = false;
  let pendiente: number | null = null;

  async function cambiarVolumen(valor: number) {
    if (volumen) volumen.nivel = valor;
    pendiente = valor;
    if (enviando) return;
    enviando = true;
    while (pendiente !== null) {
      const nivel = pendiente;
      pendiente = null;
      try {
        await invoke<Volumen>("cambiar_volumen", { nombre: config.microfono, nivel });
        errorVolumen = "";
      } catch (e) {
        errorVolumen = String(e);
      }
    }
    enviando = false;
  }

  async function activarMicrofono() {
    try {
      volumen = await invoke<Volumen>("cambiar_volumen", {
        nombre: config.microfono,
        silenciado: false,
      });
    } catch (e) {
      errorVolumen = String(e);
    }
  }

  async function intentar(accion: () => Promise<unknown>) {
    try {
      await accion();
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  const aplicar = (cambios: Partial<Config>) => intentar(() => cambiar(cambios));

  // El modelo recomendado depende de si se usa la GPU.
  const cambiarGpu = (usar_gpu: boolean) =>
    intentar(async () => {
      await cambiar({ usar_gpu });
      modelos = await invoke<Modelo[]>("listar_modelos");
    });

  const cambiarAutoarranque = (activo: boolean) =>
    intentar(async () => {
      await invoke("cambiar_autoarranque", { activo });
      autoarranque = activo;
    });

  function borrarHistorial() {
    confirmarBorrado = false;
    intentar(() => invoke("borrar_historial"));
  }

  async function buscarActualizacion() {
    actualizacion = { tipo: "buscando" };
    try {
      const nueva = await invoke<{ version: string; notas: string | null } | null>(
        "buscar_actualizacion",
      );
      actualizacion = nueva ? { tipo: "disponible", ...nueva } : { tipo: "al_dia" };
    } catch (e) {
      actualizacion = { tipo: "error", mensaje: String(e) };
    }
  }

  async function instalarActualizacion(version: string) {
    actualizacion = { tipo: "instalando", version, porcentaje: 0 };
    try {
      // Si sale bien, el instalador cierra Wister y lo vuelve a abrir: no se vuelve de acá.
      await invoke("instalar_actualizacion");
    } catch (e) {
      actualizacion = { tipo: "error", mensaje: String(e) };
    }
  }

  const abrir = (pagina: string) => intentar(() => invoke("abrir_pagina", { pagina }));

  async function exportarDiagnostico() {
    diagnostico = { tipo: "exportando" };
    try {
      diagnostico = { tipo: "listo", ruta: await invoke<string>("exportar_diagnostico") };
      error = "";
    } catch (e) {
      diagnostico = { tipo: "inactivo" };
      error = String(e);
    }
  }

  function descargar(nombre: string) {
    descargas[nombre] = { porcentaje: 0 };
    invoke("descargar_modelo", { nombre }).catch((e) => {
      descargas[nombre] = { porcentaje: null, error: String(e) };
    });
  }
</script>

<h1>Configuración</h1>
{#if error}<p class="error">{error}</p>{/if}

<section class="tarjeta">
  <h2>Modelo de voz</h2>
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
            onchange={() => aplicar({ modelo: m.nombre })}
          />
          <span class="nombre">{m.nombre}</span>
          {#if m.recomendado}<span class="insignia">Recomendado</span>{/if}
          <span class="muted derecha">{m.mb} MB</span>
        </label>
        <div class="nota muted">{m.nota}</div>
        {#if m.aviso_ram}<div class="nota aviso-ram">{m.aviso_ram}</div>{/if}
        {#if descarga && !descarga.error}
          <div class="progreso">
            <div style="width: {descarga.porcentaje ?? 0}%"></div>
          </div>
        {:else if !m.descargado}
          <button class="descargar" onclick={() => descargar(m.nombre)}>Descargar</button>
          {#if descarga?.error}<span class="error">{descarga.error}</span>{/if}
        {/if}
      </li>
    {/each}
  </ul>
  {#if gpu?.compilada}
    {@const sinPlacas = !gpu.bloqueada && gpu.placas.length === 0}
    <label class="casilla">
      <input
        type="checkbox"
        checked={config.usar_gpu && !sinPlacas}
        disabled={sinPlacas}
        onchange={(e) => cambiarGpu(e.currentTarget.checked)}
      />
      Usar la placa de video{gpu.placas.length ? ` (${gpu.placas[0]})` : ""}
    </label>
    {#if gpu.fallo_anterior && !config.usar_gpu}
      <p class="nota aviso-ram">
        La última vez Wister se cerró mientras usaba la placa de video, así que ahora usa el
        procesador. Si actualizaste los drivers, probá activarla de nuevo.
      </p>
    {/if}
    {#if gpu.bloqueada && config.usar_gpu}
      <p class="nota muted">
        Para usar la placa de video hay que reiniciar Wister.
        <button class="enlace" onclick={() => invoke("reiniciar")}>Reiniciar ahora</button>
      </p>
    {:else if sinPlacas}
      <p class="nota muted">
        No se encontró una placa de video compatible: Wister usa el procesador.
      </p>
    {/if}
  {/if}
</section>

<section class="tarjeta">
  <h2>Dictado</h2>
  <div class="fila">
    <label for="microfono">Micrófono</label>
    <select
      id="microfono"
      value={config.microfono ?? ""}
      onchange={(e) => aplicar({ microfono: e.currentTarget.value || null })}
    >
      <option value="">
        Predeterminado de Windows{microfonoPredeterminado ? ` (${microfonoPredeterminado})` : ""}
      </option>
      {#each microfonos as m}
        <option value={m.nombre}>{m.nombre}</option>
      {/each}
      {#if config.microfono && !microfonos.some((m) => m.nombre === config.microfono)}
        <option value={config.microfono}>{config.microfono} (no conectado)</option>
      {/if}
    </select>
  </div>

  <div class="fila">
    <label for="volumen">Volumen</label>
    <input
      id="volumen"
      type="range"
      min="0"
      max="100"
      disabled={!volumen}
      value={Math.round((volumen?.nivel ?? 0) * 100)}
      oninput={(e) => cambiarVolumen(+e.currentTarget.value / 100)}
    />
    <span class="porcentaje">{volumen ? `${Math.round(volumen.nivel * 100)}%` : ""}</span>
    {#if probando}
      <button onclick={dejarDeProbar}>Dejar de probar</button>
    {:else}
      <button onclick={probar}>Probar</button>
    {/if}
  </div>
  <div class="medidor" class:apagado={!probando} title="Nivel del micrófono">
    <div style="width: {nivel * 100}%"></div>
  </div>
  {#if volumen?.silenciado}
    <p class="aviso">
      El micrófono está silenciado en Windows: así no se escucha nada.
      <button onclick={activarMicrofono}>Activar</button>
    </p>
  {/if}
  {#if errorVolumen}<p class="error">{errorVolumen}</p>{/if}
  <p class="muted nota-volumen">
    El volumen es el mismo de la configuración de sonido de Windows: cambia para todas las apps.
  </p>

  <div class="fila">
    <label for="idioma">Idioma</label>
    <select
      id="idioma"
      value={config.idioma}
      onchange={(e) => aplicar({ idioma: e.currentTarget.value })}
    >
      {#each IDIOMAS as [codigo, nombre]}
        <option value={codigo}>{nombre}</option>
      {/each}
    </select>
  </div>

  <div class="fila">
    <span class="etiqueta">Atajo</span>
    <CapturaAtajo atajo={config.atajo} oncambiar={(atajo) => aplicar({ atajo })} />
  </div>

  <label class="casilla">
    <input
      type="checkbox"
      checked={config.mostrar_overlay}
      onchange={(e) => aplicar({ mostrar_overlay: e.currentTarget.checked })}
    />
    Mostrar la onda mientras grabo
  </label>
  <label class="casilla">
    <input
      type="checkbox"
      checked={config.sonidos}
      onchange={(e) => aplicar({ sonidos: e.currentTarget.checked })}
    />
    Sonido al empezar y al terminar de grabar
  </label>
  <label class="casilla">
    <input
      type="checkbox"
      checked={config.sin_punto_final}
      onchange={(e) => aplicar({ sin_punto_final: e.currentTarget.checked })}
    />
    No poner punto al final (queda más natural en los chats)
  </label>
  <label class="casilla">
    <input
      type="checkbox"
      checked={autoarranque}
      onchange={(e) => cambiarAutoarranque(e.currentTarget.checked)}
    />
    Iniciar Wister con Windows
  </label>
</section>

<section class="tarjeta">
  <h2>Historial</h2>
  <label class="casilla">
    <input
      type="checkbox"
      checked={config.guardar_historial}
      onchange={(e) => aplicar({ guardar_historial: e.currentTarget.checked })}
    />
    Guardar el historial de dictados en esta PC
  </label>
  <p class="muted">
    Se guarda solo el texto, nunca el audio. Las estadísticas de Inicio salen del historial.
  </p>
  <div class="borrar">
    {#if confirmarBorrado}
      <span>¿Borrar todos los dictados? No se puede deshacer.</span>
      <button class="peligro" onclick={borrarHistorial}>Sí, borrar todo</button>
      <button onclick={() => (confirmarBorrado = false)}>Cancelar</button>
    {:else}
      <button onclick={() => (confirmarBorrado = true)}>Borrar todo el historial</button>
    {/if}
  </div>
</section>

<section class="tarjeta">
  <h2>Diagnóstico</h2>
  <h3>Prueba de rendimiento</h3>
  <Rendimiento modeloActual={modeloElegido} usarModelo={(modelo) => aplicar({ modelo })} />

  <h3>Informe para soporte</h3>
  <p class="muted">
    Guarda en Descargas un archivo de texto con los datos de la PC, la configuración y el registro
    de funcionamiento de Wister. No incluye nada de lo que dictaste.
  </p>
  <button disabled={diagnostico.tipo === "exportando"} onclick={exportarDiagnostico}>
    {diagnostico.tipo === "exportando" ? "Exportando…" : "Exportar diagnóstico"}
  </button>
  {#if diagnostico.tipo === "listo"}
    <p class="muted">Guardado en <code>{diagnostico.ruta}</code></p>
  {/if}
</section>

<section class="tarjeta">
  <h2>Acerca de Wister</h2>
  <p class="acerca">
    Wister es un proyecto de <strong>Diego Depolo</strong> (y Claude =P). Es de software libre para
    la comunidad (licencia GPL-3.0): podés usarlo, estudiar el código, modificarlo, compartirlo o
    sumarte al proyecto siempre que lo que hagas a partir de él también sea libre.
  </p>
  <p class="acerca muted">
    Transcribe con Whisper, de OpenAI, a través de whisper.cpp, y detecta la voz con Silero VAD.
    Todo corre en tu PC.
  </p>
  <div class="enlaces">
    <button class="enlace" onclick={() => abrir("repositorio")}>Código fuente</button>
    <button class="enlace" onclick={() => abrir("problemas")}>Reportar un problema</button>
    <button class="enlace" onclick={() => abrir("licencia")}>Licencia</button>
  </div>
  <div class="version">
    <span>Versión {version}</span>
    {#if actualizacion.tipo === "inactiva" || actualizacion.tipo === "al_dia" || actualizacion.tipo === "error"}
      <button onclick={buscarActualizacion}>Buscar actualizaciones</button>
    {:else if actualizacion.tipo === "buscando"}
      <button disabled>Buscando…</button>
    {/if}
  </div>

  {#if actualizacion.tipo === "al_dia"}
    <p class="muted">Tenés la última versión.</p>
  {:else if actualizacion.tipo === "error"}
    <p class="error">{actualizacion.mensaje}</p>
  {:else if actualizacion.tipo === "disponible"}
    {@const nueva = actualizacion.version}
    <div class="nueva">
      <strong>Hay una versión nueva: {actualizacion.version}</strong>
      {#if actualizacion.notas}<pre class="notas">{actualizacion.notas}</pre>{/if}
      <button class="principal" onclick={() => instalarActualizacion(nueva)}>
        Actualizar ahora
      </button>
      <span class="muted">Wister se cierra, instala la versión nueva y se vuelve a abrir.</span>
    </div>
  {:else if actualizacion.tipo === "instalando"}
    <p>Bajando la versión {actualizacion.version}…</p>
    <div class="progreso ancho">
      <div style="width: {actualizacion.porcentaje ?? 0}%"></div>
    </div>
  {/if}

  <p class="muted">
    Solo se conecta a GitHub cuando tocás el botón. Cada versión viene firmada y se verifica
    antes de instalarla.
  </p>
  <button class="enlace" onclick={() => aplicar({ asistente_completo: false })}>
    Volver a ejecutar el asistente
  </button>
</section>

<style>
  .tarjeta {
    margin-top: 16px;
  }
  h3 {
    margin: 14px 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  h2 + h3 {
    margin-top: 0;
  }
  code {
    font-size: 12px;
    word-break: break-all;
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
  .modelos li:last-child {
    margin-bottom: 0;
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
  .aviso-ram {
    color: var(--naranja);
  }
  .descargar {
    margin: 6px 0 0 24px;
  }
  .progreso {
    margin: 8px 0 2px 24px;
    height: 6px;
    border-radius: 3px;
    background: var(--suave);
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
    margin-bottom: 10px;
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
  input[type="range"] {
    flex: 1;
    min-width: 0;
    accent-color: var(--acento);
  }
  .porcentaje {
    width: 36px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .medidor {
    height: 6px;
    margin: -2px 0 8px 90px;
    border-radius: 3px;
    background: var(--suave);
    overflow: hidden;
  }
  .medidor.apagado {
    opacity: 0.5;
  }
  .medidor div {
    height: 100%;
    background: var(--acento);
    transition: width 60ms linear;
  }
  .aviso {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px 90px;
    font-size: 13px;
    color: var(--naranja);
  }
  .nota-volumen {
    margin: 0 0 12px 90px;
    font-size: 12px;
  }
  .casilla {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: 13px;
  }
  .borrar {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .peligro {
    border-color: var(--rojo);
    color: var(--rojo);
  }
  .acerca {
    margin: 0 0 8px;
    font-size: 13px;
    line-height: 1.5;
  }
  .enlaces {
    display: flex;
    gap: 16px;
    margin-bottom: 14px;
  }
  .version {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .nueva {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    margin-top: 12px;
    padding: 12px;
    border: 1px solid var(--acento);
    border-radius: 8px;
    font-size: 13px;
  }
  .notas {
    box-sizing: border-box;
    width: 100%;
    max-height: 180px;
    margin: 0;
    overflow-y: auto;
    font-family: inherit;
    font-size: 12px;
    white-space: pre-wrap;
    opacity: 0.85;
  }
  .principal {
    background: var(--acento);
    border-color: var(--acento);
    color: #fff;
  }
  .progreso.ancho {
    margin: 0 0 8px;
  }
</style>
