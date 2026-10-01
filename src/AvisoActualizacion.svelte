<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  type Disponible = { version: string; notas: string | null };

  /** Versión que el usuario dejó para después: no se vuelve a ofrecer hasta que salga otra. */
  const DESCARTADA = "wister.actualizacion.descartada";

  let disponible = $state<Disponible | null>(null);
  let instalando = $state(false);
  let porcentaje = $state<number | null>(null);
  let error = $state("");

  function leerDescartada(): string | null {
    try {
      return localStorage.getItem(DESCARTADA);
    } catch {
      return null;
    }
  }

  function mostrar(d: Disponible | null) {
    if (d && d.version !== leerDescartada()) disponible = d;
  }

  onMount(() => {
    invoke<Disponible | null>("actualizacion_pendiente").then(mostrar);
    const aviso = listen<Disponible>("actualizacion_disponible", ({ payload }) => mostrar(payload));
    const progreso = listen<{ bajado: number; total: number | null }>(
      "actualizacion",
      ({ payload }) => {
        porcentaje = payload.total ? (payload.bajado / payload.total) * 100 : null;
      },
    );
    return () => {
      aviso.then((dejar) => dejar());
      progreso.then((dejar) => dejar());
    };
  });

  function despues() {
    if (!disponible) return;
    try {
      localStorage.setItem(DESCARTADA, disponible.version);
    } catch {
      // Sin almacenamiento, el cartel vuelve a aparecer la próxima vez: no es grave.
    }
    disponible = null;
  }

  async function actualizar() {
    instalando = true;
    error = "";
    try {
      // Si sale bien no vuelve: el instalador cierra Wister y lo abre de nuevo.
      await invoke("instalar_actualizacion");
    } catch (e) {
      error = String(e);
      instalando = false;
    }
  }
</script>

{#if disponible}
  <div class="aviso-actualizacion">
    <div class="texto">
      <strong>Hay una versión nueva de Wister: {disponible.version}</strong>
      {#if instalando}
        <span class="muted">Bajando… Wister se va a cerrar y abrir solo.</span>
        <div class="progreso"><div style="width: {porcentaje ?? 0}%"></div></div>
      {:else}
        <span class="muted">Se instala en unos segundos y no perdés nada de tu configuración.</span>
      {/if}
      {#if error}<span class="error">{error}</span>{/if}
    </div>
    {#if !instalando}
      <button class="principal" onclick={actualizar}>Actualizar ahora</button>
      <button onclick={despues}>Ahora no</button>
    {/if}
  </div>
{/if}

<style>
  .aviso-actualizacion {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 16px;
    padding: 12px 14px;
    border: 1px solid var(--acento);
    border-radius: 10px;
    background: var(--superficie);
  }
  .texto {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
  }
  .principal {
    background: var(--acento);
    border-color: var(--acento);
    color: #fff;
  }
  .progreso {
    height: 6px;
    margin-top: 6px;
    border-radius: 3px;
    background: var(--suave);
    overflow: hidden;
  }
  .progreso div {
    height: 100%;
    background: var(--acento);
    transition: width 60ms linear;
  }
</style>
