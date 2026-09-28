<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy } from "svelte";
  import type { EventoRendimiento, Medicion } from "./tipos";

  let {
    modeloActual,
    usarModelo,
  }: { modeloActual: string; usarModelo: (modelo: string) => void } = $props();

  type Fase =
    | { tipo: "inactiva" }
    | { tipo: "grabando"; frase: string }
    | { tipo: "midiendo"; modelo: string; gpu: boolean; paso: number; total: number }
    | { tipo: "lista"; recomendado: string | null }
    | { tipo: "error"; mensaje: string };

  let fase = $state<Fase>({ tipo: "inactiva" });
  let mediciones = $state<Medicion[]>([]);
  let nivel = $state(0);

  /** Modelos con los que la CPU le ganó a la GPU (para saber si conviene poder elegir). */
  const cpuMasRapida = $derived(
    mediciones
      .filter(
        (c) =>
          !c.gpu &&
          !c.error &&
          mediciones.some(
            (g) =>
              g.gpu && !g.error && g.modelo === c.modelo && g.transcripcion_ms > c.transcripcion_ms,
          ),
      )
      .map((c) => c.modelo),
  );

  const escuchas = [
    listen<number>("nivel_prueba", ({ payload }) => {
      if (fase.tipo === "grabando") nivel = Math.min(1, Math.sqrt(payload / 0.12));
    }),
    listen<EventoRendimiento>("rendimiento", ({ payload }) => {
      switch (payload.tipo) {
        case "midiendo":
          fase = { ...payload, tipo: "midiendo" };
          break;
        case "medicion": {
          const { tipo: _, ...m } = payload;
          mediciones.push(m);
          break;
        }
        case "fin":
          fase = { tipo: "lista", recomendado: payload.recomendado };
          break;
        case "error":
          fase = { tipo: "error", mensaje: payload.mensaje };
      }
    }),
  ];

  onDestroy(() => {
    escuchas.forEach((e) => e.then((dejar) => dejar()));
    if (fase.tipo === "grabando") invoke("cancelar_prueba_rendimiento");
  });

  async function empezar() {
    mediciones = [];
    nivel = 0;
    try {
      const frase = await invoke<string>("iniciar_prueba_rendimiento");
      fase = { tipo: "grabando", frase };
    } catch (e) {
      fase = { tipo: "error", mensaje: String(e) };
    }
  }

  function terminar() {
    fase = { tipo: "midiendo", modelo: "", gpu: false, paso: 0, total: 0 };
    invoke("medir_prueba_rendimiento").catch((e) => (fase = { tipo: "error", mensaje: String(e) }));
  }

  function cancelar() {
    invoke("cancelar_prueba_rendimiento");
    fase = { tipo: "inactiva" };
  }

  const tiempo = (ms: number) =>
    ms < 1000 ? `${ms} ms` : `${(ms / 1000).toLocaleString("es-AR", { maximumFractionDigits: 1 })} s`;
</script>

{#if fase.tipo === "inactiva" || fase.tipo === "error"}
  <p class="muted">
    Leés una frase en voz alta y Wister la transcribe con cada modelo que tenés descargado, para
    ver cuál anda mejor en esta PC. Puede tardar un par de minutos, y mientras tanto no se puede
    dictar.
  </p>
  {#if fase.tipo === "error"}<p class="error">{fase.mensaje}</p>{/if}
  <button onclick={empezar}>Empezar la prueba</button>
{:else if fase.tipo === "grabando"}
  <p>Leé esto en voz alta, a tu ritmo normal:</p>
  <blockquote>{fase.frase}</blockquote>
  <div class="medidor"><div style="width: {nivel * 100}%"></div></div>
  <div class="botones">
    <button class="principal" onclick={terminar}>Terminé</button>
    <button onclick={cancelar}>Cancelar</button>
  </div>
{:else if fase.tipo === "midiendo"}
  <p>
    {#if fase.total}
      Probando {fase.modelo} con {fase.gpu ? "GPU" : "CPU"} ({fase.paso} de {fase.total})…
    {:else}
      Preparando…
    {/if}
  </p>
  <div class="progreso">
    <div style="width: {fase.total ? ((fase.paso - 1) / fase.total) * 100 : 0}%"></div>
  </div>
{/if}

{#if mediciones.length}
  <table>
    <thead>
      <tr>
        <th>Modelo</th>
        <th>Con</th>
        <th title="Lo que se espera después de soltar el atajo">Espera</th>
        <th title="Palabras de la frase que transcribió bien">Aciertos</th>
        <th title="Lo que tarda en cargarse al elegirlo">Carga</th>
      </tr>
    </thead>
    <tbody>
      {#each mediciones as m}
        <tr title={m.error ?? m.texto}>
          <td>{m.modelo}</td>
          <td>{m.gpu ? "GPU" : "CPU"}</td>
          {#if m.error}
            <td colspan="3" class="error">falló</td>
          {:else}
            <td>{tiempo(m.transcripcion_ms)}</td>
            <td>{m.aciertos}%</td>
            <td>{tiempo(m.carga_ms)}</td>
          {/if}
        </tr>
      {/each}
    </tbody>
  </table>
  <details>
    <summary>Lo que transcribió cada uno</summary>
    <ul>
      {#each mediciones as m}
        <li>
          <strong>{m.modelo} ({m.gpu ? "GPU" : "CPU"}):</strong>
          {m.error ?? m.texto}
        </li>
      {/each}
    </ul>
  </details>
{/if}

{#if fase.tipo === "lista"}
  {#if fase.recomendado}
    {@const recomendado = fase.recomendado}
    <div class="recomendacion">
      {#if recomendado === modeloActual}
        <span>El modelo que usás, <strong>{recomendado}</strong>, es el que mejor anda en esta PC.</span>
      {:else}
        <span>Para esta PC te conviene <strong>{recomendado}</strong>.</span>
        <button class="principal" onclick={() => usarModelo(recomendado)}>Usar {recomendado}</button>
      {/if}
    </div>
  {/if}
  {#if cpuMasRapida.length}
    <p class="muted">
      Con {cpuMasRapida.join(", ")}, la CPU fue más rápida que la GPU. Si exportás el diagnóstico,
      queda anotado.
    </p>
  {/if}
  <button onclick={empezar}>Repetir la prueba</button>
{/if}

<style>
  p {
    font-size: 13px;
  }
  blockquote {
    margin: 8px 0;
    padding: 10px 14px;
    border-left: 3px solid var(--acento);
    background: var(--suave);
    border-radius: 0 6px 6px 0;
    font-size: 15px;
    line-height: 1.5;
  }
  .medidor,
  .progreso {
    height: 8px;
    margin: 10px 0;
    border-radius: 4px;
    background: var(--suave);
    overflow: hidden;
  }
  .medidor div,
  .progreso div {
    height: 100%;
    background: var(--acento);
  }
  .medidor div {
    transition: width 60ms linear;
  }
  .progreso div {
    transition: width 300ms ease;
  }
  .botones {
    display: flex;
    gap: 8px;
  }
  .principal {
    background: var(--acento);
    border-color: var(--acento);
    color: #fff;
  }
  table {
    width: 100%;
    margin: 12px 0 6px;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    padding: 5px 8px;
    text-align: left;
    border-bottom: 1px solid var(--borde);
  }
  th {
    font-weight: 600;
    cursor: help;
  }
  th:first-child {
    cursor: default;
  }
  details {
    margin-bottom: 10px;
    font-size: 12px;
  }
  summary {
    cursor: pointer;
  }
  details ul {
    margin: 6px 0 0;
    padding-left: 18px;
  }
  details li {
    margin-bottom: 4px;
  }
  .recomendacion {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 10px 0;
    padding: 10px 12px;
    border: 1px solid var(--acento);
    border-radius: 8px;
    font-size: 13px;
  }
  .recomendacion button {
    margin-left: auto;
  }
</style>
