<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import Historial from "./Historial.svelte";
  import {
    DESCARTES,
    noPegado,
    nombreAtajo,
    type Config,
    type Estadisticas,
    type Estado,
    type Resultado,
  } from "./tipos";

  let {
    config,
    estado,
    ultimo,
    onvertodo,
  }: {
    config: Config;
    estado: Estado;
    ultimo: Resultado | null;
    onvertodo: () => void;
  } = $props();

  let estadisticas = $state<Estadisticas | null>(null);
  let error = $state("");

  function cargar() {
    invoke<Estadisticas>("estadisticas")
      .then((e) => {
        estadisticas = e;
        error = "";
      })
      .catch((e) => (error = `No se pudieron calcular las estadísticas: ${e}`));
  }

  onMount(() => {
    cargar();
    // Cualquier cambio en el historial (dictado nuevo, borrado) cambia los números.
    const escucha = listen("historial", cargar);
    // Pasada la medianoche, "hoy" y la racha cambian aunque no se dicte.
    const reloj = setInterval(cargar, 10 * 60 * 1000);
    return () => {
      clearInterval(reloj);
      escucha.then((dejar) => dejar());
    };
  });

  const numero = (n: number) => n.toLocaleString("es-AR");

  function duracion(segundos: number): string {
    if (segundos < 60) return `${segundos} s`;
    const minutos = Math.round(segundos / 60);
    if (minutos < 60) return `${minutos} min`;
    const horas = Math.floor(minutos / 60);
    const resto = minutos % 60;
    return resto ? `${horas} h ${resto} min` : `${horas} h`;
  }

  const plural = (n: number, uno: string, varios: string) => (n === 1 ? uno : varios);

  const tarjetas = $derived(
    estadisticas && [
      {
        titulo: "Hoy",
        valor: numero(estadisticas.palabras_hoy),
        nota: "palabras",
      },
      {
        titulo: "Últimos 7 días",
        valor: numero(estadisticas.palabras_semana),
        nota: "palabras",
      },
      {
        titulo: "En total",
        valor: numero(estadisticas.palabras_total),
        nota: `palabras en ${numero(estadisticas.dictados_total)} ${plural(estadisticas.dictados_total, "dictado", "dictados")}`,
      },
      {
        titulo: "Tiempo ahorrado",
        valor: duracion(estadisticas.segundos_ahorrados),
        nota: "contra tipear a 40 palabras por minuto",
      },
      {
        titulo: "Racha",
        valor: `${estadisticas.racha_dias} ${plural(estadisticas.racha_dias, "día", "días")}`,
        nota: estadisticas.racha_dias ? "seguidos dictando" : "dictá hoy para empezar una",
      },
      {
        titulo: "Velocidad",
        valor: numero(estadisticas.palabras_por_minuto),
        nota: "palabras por minuto hablando",
      },
    ],
  );
</script>

<h1>Inicio</h1>
<p class="ayuda">Mantené apretado <kbd>{nombreAtajo(config.atajo)}</kbd>, hablá y soltá.</p>

{#if estado.tipo === "error"}
  <p class="error">{estado.mensaje}</p>
{/if}
{#if ultimo?.tipo === "descartado"}
  <p class="muted">Último dictado descartado: {DESCARTES[ultimo.motivo]}.</p>
{:else if ultimo?.tipo === "texto" && noPegado(ultimo.pegado)}
  <p class="aviso">{noPegado(ultimo.pegado)}</p>
{/if}
{#if error}<p class="error">{error}</p>{/if}

{#if tarjetas}
  <div class="tarjetas">
    {#each tarjetas as t}
      <div class="tarjeta">
        <div class="titulo">{t.titulo}</div>
        <div class="valor">{t.valor}</div>
        {#if t.nota}<div class="muted">{t.nota}</div>{/if}
      </div>
    {/each}
  </div>
{/if}
{#if !config.guardar_historial}
  <p class="muted">
    El historial está desactivado, así que los dictados nuevos no suman a las estadísticas.
  </p>
{/if}

<section class="tarjeta ultimos">
  <div class="encabezado">
    <h2>Últimos dictados</h2>
    <button class="enlace" onclick={onvertodo}>Ver todo el historial</button>
  </div>
  <Historial guardando={config.guardar_historial} resumen />
</section>

<style>
  .ayuda {
    margin: 0 0 16px;
    font-size: 14px;
  }
  .tarjetas {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
  }
  .titulo {
    font-size: 12px;
    opacity: 0.7;
  }
  .valor {
    margin: 4px 0 2px;
    font-size: 26px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .ultimos {
    margin-top: 16px;
  }
  .encabezado {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
</style>
