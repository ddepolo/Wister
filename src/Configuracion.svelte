<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import CapturaAtajo from "./CapturaAtajo.svelte";
  import { IDIOMAS, type Config, type Descarga, type Microfono, type Modelo } from "./tipos";

  let {
    config,
    cambiar,
  }: { config: Config; cambiar: (cambios: Partial<Config>) => Promise<void> } = $props();

  let version = $state("");
  let modelos = $state<Modelo[]>([]);
  let microfonos = $state<Microfono[]>([]);
  let descargas = $state<Record<string, { porcentaje: number | null; error?: string }>>({});
  let autoarranque = $state(false);
  let confirmarBorrado = $state(false);
  let error = $state("");

  const recomendado = $derived(modelos.find((m) => m.recomendado)?.nombre ?? "");
  const modeloElegido = $derived(config.modelo ?? recomendado);
  const microfonoPredeterminado = $derived(microfonos.find((m) => m.predeterminado)?.nombre);

  onMount(() => {
    getVersion().then((v) => (version = v));
    invoke<boolean>("autoarranque").then((a) => (autoarranque = a));
    refrescar();
    // Al volver a la ventana se releen los micrófonos: puede que se haya enchufado uno.
    window.addEventListener("focus", refrescar);
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
      escucha.then((dejar) => dejar());
    };
  });

  function refrescar() {
    invoke<Modelo[]>("listar_modelos").then((m) => (modelos = m));
    invoke<Microfono[]>("listar_microfonos")
      .then((m) => (microfonos = m))
      .catch((e) => (error = String(e)));
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

  const cambiarAutoarranque = (activo: boolean) =>
    intentar(async () => {
      await invoke("cambiar_autoarranque", { activo });
      autoarranque = activo;
    });

  function borrarHistorial() {
    confirmarBorrado = false;
    intentar(() => invoke("borrar_historial"));
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

<p class="muted pie">
  Wister {version} ·
  <button class="enlace" onclick={() => aplicar({ asistente_completo: false })}>
    Volver a ejecutar el asistente
  </button>
</p>

<style>
  .tarjeta {
    margin-top: 16px;
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
  .pie {
    margin-top: 16px;
  }
</style>
