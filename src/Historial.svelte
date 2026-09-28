<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import type { CambioHistorial, Dictado } from "./tipos";

  // `resumen`: los últimos dictados para Inicio, sin buscador ni paginado.
  let { guardando, resumen = false }: { guardando: boolean; resumen?: boolean } = $props();

  const PAGINA = $derived(resumen ? 5 : 50);

  let dictados = $state<Dictado[]>([]);
  let busqueda = $state("");
  let hayMas = $state(false);
  let cargado = $state(false);
  let error = $state("");
  let copiado = $state<number | null>(null);
  // Cada consulta lleva un número: si llega la respuesta de una búsqueda vieja, se ignora.
  let consulta = 0;
  let demora: ReturnType<typeof setTimeout> | undefined;

  async function cargar(mas = false) {
    const numero = ++consulta;
    try {
      const nuevos = await invoke<Dictado[]>("listar_historial", {
        busqueda,
        antesDe: mas ? (dictados.at(-1)?.id ?? null) : null,
        limite: PAGINA,
      });
      if (numero !== consulta) return;
      dictados = mas ? [...dictados, ...nuevos] : nuevos;
      hayMas = !resumen && nuevos.length === PAGINA;
      cargado = true;
      error = "";
    } catch (e) {
      if (numero === consulta) error = String(e);
    }
  }

  function buscar() {
    clearTimeout(demora);
    demora = setTimeout(() => cargar(), 200);
  }

  onMount(() => {
    cargar();
    const escucha = listen<CambioHistorial>("historial", ({ payload }) => {
      switch (payload.tipo) {
        case "agregado":
          // Con una búsqueda activa no se sabe si el nuevo coincide: se vuelve a pedir.
          if (busqueda.trim()) cargar();
          else if (resumen) dictados = [payload.dictado, ...dictados].slice(0, PAGINA);
          else dictados = [payload.dictado, ...dictados];
          error = "";
          break;
        case "borrado":
          // En el resumen se vuelve a pedir, para que siga mostrando los últimos cinco.
          if (resumen) cargar();
          else dictados = dictados.filter((d) => d.id !== payload.id);
          break;
        case "borrado_todo":
          dictados = [];
          hayMas = false;
          break;
        case "error":
          error = payload.mensaje;
          break;
      }
    });
    return () => {
      clearTimeout(demora);
      escucha.then((dejar) => dejar());
    };
  });

  async function copiar(d: Dictado) {
    try {
      await navigator.clipboard.writeText(d.texto);
      copiado = d.id;
      setTimeout(() => {
        if (copiado === d.id) copiado = null;
      }, 1500);
    } catch (e) {
      error = `No se pudo copiar: ${e}`;
    }
  }

  async function borrar(d: Dictado) {
    try {
      await invoke("borrar_dictado", { id: d.id });
    } catch (e) {
      error = String(e);
    }
  }

  const hora = (fecha: number) =>
    new Date(fecha).toLocaleTimeString("es-AR", { hour: "2-digit", minute: "2-digit" });

  function dia(fecha: number): string {
    const d = new Date(fecha);
    const hoy = new Date();
    const ayer = new Date(hoy.getFullYear(), hoy.getMonth(), hoy.getDate() - 1);
    if (d.toDateString() === hoy.toDateString()) return "Hoy";
    if (d.toDateString() === ayer.toDateString()) return "Ayer";
    return d.toLocaleDateString("es-AR", {
      weekday: "long",
      day: "numeric",
      month: "long",
      year: d.getFullYear() === hoy.getFullYear() ? undefined : "numeric",
    });
  }

  const seg = (ms: number) => (ms / 1000).toFixed(1).replace(".", ",") + " s";
</script>

{#if !resumen}
  <input
    class="buscador"
    type="search"
    placeholder="Buscar en el historial"
    bind:value={busqueda}
    oninput={buscar}
  />
{/if}

{#if !guardando && !resumen}
  <p class="muted">
    El historial está desactivado: los dictados nuevos no se guardan. Podés activarlo en
    Configuración.
  </p>
{/if}
{#if error}<p class="error">{error}</p>{/if}

<ul class="historial">
  {#each dictados as d, i (d.id)}
    {#if i === 0 || dia(d.fecha) !== dia(dictados[i - 1].fecha)}
      <li class="dia">{dia(d.fecha)}</li>
    {/if}
    <li class="dictado">
      <p class="texto">{d.texto}</p>
      <div class="pie">
        <span class="muted">
          {hora(d.fecha)} · {d.palabras}
          {d.palabras === 1 ? "palabra" : "palabras"} · {seg(d.audio_ms)}{#if d.app}
            · <span class="app" title={d.app}>{d.app}</span>{/if}
        </span>
        <span class="acciones">
          <button onclick={() => copiar(d)}>{copiado === d.id ? "Copiado" : "Copiar"}</button>
          <button onclick={() => borrar(d)}>Borrar</button>
        </span>
      </div>
    </li>
  {:else}
    {#if cargado}
      <li class="muted vacio">
        {busqueda.trim() ? "No hay dictados que coincidan." : "Todavía no hay dictados guardados."}
      </li>
    {/if}
  {/each}
</ul>

{#if hayMas}
  <button class="mas" onclick={() => cargar(true)}>Ver más</button>
{/if}

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 13px;
  }
  .buscador {
    box-sizing: border-box;
    width: 100%;
    padding: 7px 12px;
    margin-bottom: 4px;
  }
  .dia {
    padding: 12px 0 4px;
    font-size: 12px;
    font-weight: 600;
    opacity: 0.7;
  }
  .dia:first-child {
    padding-top: 4px;
  }
  .dia::first-letter {
    text-transform: uppercase;
  }
  .dictado {
    padding: 8px 0;
    border-bottom: 1px solid var(--borde);
  }
  .dictado:last-child {
    border-bottom: none;
  }
  .texto {
    margin: 0 0 4px;
    font-size: 15px;
    white-space: pre-wrap;
    user-select: text;
  }
  .pie {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pie .muted {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acciones {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .acciones button {
    padding: 2px 8px;
    font-size: 12px;
  }
  .vacio {
    padding: 8px 0;
  }
  .mas {
    margin-top: 10px;
  }
</style>
