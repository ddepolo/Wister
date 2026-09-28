<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  // Como archivo aparte: si Vite lo embebiera como `data:`, la CSP no lo dejaría cargar.
  import logo from "../src-tauri/icons/icon.svg?no-inline";
  import Asistente from "./Asistente.svelte";
  import Configuracion from "./Configuracion.svelte";
  import Diccionario from "./Diccionario.svelte";
  import Historial from "./Historial.svelte";
  import Icono from "./Icono.svelte";
  import Inicio from "./Inicio.svelte";
  import type { Config, Estado, Resultado } from "./tipos";

  type Seccion = "inicio" | "historial" | "diccionario" | "configuracion";

  const SECCIONES: [Seccion, string][] = [
    ["inicio", "Inicio"],
    ["historial", "Historial"],
    ["diccionario", "Diccionario"],
    ["configuracion", "Configuración"],
  ];

  let seccion = $state<Seccion>("inicio");
  let estado = $state<Estado>({ tipo: "iniciando" });
  // Solo para avisar si el último dictado se descartó o no se pudo pegar.
  let ultimo = $state<Resultado | null>(null);
  let config = $state<Config | null>(null);
  let errorConfig = $state("");
  let confirmarSalida = $state(false);

  invoke<Estado>("estado_actual").then((e) => (estado = e));
  invoke<Config>("obtener_config")
    .then((c) => (config = c))
    .catch((e) => (errorConfig = `No se pudo leer la configuración: ${e}`));

  listen<Estado>("estado", ({ payload }) => (estado = payload));
  listen<Resultado>("resultado", ({ payload }) => (ultimo = payload));

  /** Guarda la configuración; si falla, tira el error para que lo muestre quien llamó. */
  async function cambiar(cambios: Partial<Config>) {
    if (!config) return;
    const nueva = { ...config, ...cambios };
    await invoke("guardar_config", { config: nueva });
    config = nueva;
  }

  const etiqueta = $derived(
    {
      iniciando: "Iniciando…",
      cargando_modelo: "Cargando modelo…",
      listo: "En espera",
      grabando: "Grabando",
      transcribiendo: "Transcribiendo…",
      error: "Error",
    }[estado.tipo],
  );
</script>

{#if config && !config.asistente_completo}
  <Asistente
    {config}
    onterminar={(c) => {
      config = c;
      seccion = "inicio";
    }}
  />
{:else if config}
  <div class="app">
    <nav>
      <div class="marca">
        <img src={logo} alt="" width="28" height="28" />
        <span>Wister</span>
      </div>
      {#each SECCIONES as [id, nombre]}
        <button class="item" class:activo={seccion === id} onclick={() => (seccion = id)}>
          <Icono nombre={id} />
          {nombre}
        </button>
      {/each}

      <div class="estado {estado.tipo}" title={estado.tipo === "error" ? estado.mensaje : ""}>
        <span class="punto"></span>
        <span>
          {etiqueta}
          {#if estado.tipo === "listo"}
            <span class="muted">{estado.modelo} · {estado.gpu ? "GPU" : "CPU"}</span>
          {:else if estado.tipo === "cargando_modelo"}
            <span class="muted">{estado.modelo}</span>
          {/if}
        </span>
      </div>
      {#if confirmarSalida}
        <div class="confirmar">
          <p>¿Cerrar Wister? El atajo deja de funcionar hasta que lo vuelvas a abrir.</p>
          <div class="botones">
            <button class="peligro" onclick={() => invoke("salir")}>Salir</button>
            <button onclick={() => (confirmarSalida = false)}>Cancelar</button>
          </div>
        </div>
      {:else}
        <button class="item" onclick={() => (confirmarSalida = true)}>
          <Icono nombre="salir" />
          Salir de Wister
        </button>
      {/if}
    </nav>

    <!-- Las secciones quedan montadas: así no se pierden la búsqueda ni las descargas en curso. -->
    <main>
      <div class="seccion" hidden={seccion !== "inicio"}>
        <Inicio {config} {estado} {ultimo} onvertodo={() => (seccion = "historial")} />
      </div>
      <div class="seccion" hidden={seccion !== "historial"}>
        <h1>Historial</h1>
        <Historial guardando={config.guardar_historial} />
      </div>
      <div class="seccion" hidden={seccion !== "diccionario"}>
        <Diccionario />
      </div>
      <div class="seccion" hidden={seccion !== "configuracion"}>
        <Configuracion {config} {cambiar} />
      </div>
    </main>
  </div>
{:else if errorConfig}
  <p class="error">{errorConfig}</p>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: 100vh;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 10px;
    background: var(--lateral);
    border-right: 1px solid var(--borde);
  }
  .marca {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px 18px;
    font-size: 17px;
    font-weight: 600;
  }
  .marca img {
    border-radius: 7px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: none;
    font-size: 14px;
    text-align: left;
    opacity: 0.75;
  }
  .item:hover {
    background: var(--suave);
    opacity: 1;
  }
  .item.activo {
    background: var(--suave);
    color: var(--acento);
    font-weight: 600;
    opacity: 1;
  }
  .estado {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: auto;
    padding: 10px;
    font-size: 13px;
  }
  .estado .muted {
    display: block;
    margin-top: 2px;
  }
  .punto {
    flex: none;
    width: 8px;
    height: 8px;
    margin-top: 5px;
    border-radius: 50%;
    background: var(--borde);
  }
  .listo .punto {
    background: var(--verde);
  }
  .grabando .punto {
    background: var(--rojo);
  }
  .transcribiendo .punto,
  .cargando_modelo .punto {
    background: var(--acento);
  }
  .estado.error {
    color: var(--rojo);
  }
  .error .punto {
    background: var(--rojo);
  }
  .confirmar {
    padding: 10px;
    border: 1px solid var(--borde);
    border-radius: 8px;
    background: var(--superficie);
    font-size: 12px;
  }
  .confirmar p {
    margin: 0 0 8px;
    line-height: 1.4;
  }
  .botones {
    display: flex;
    gap: 6px;
  }
  .botones button {
    flex: 1;
  }
  .peligro {
    border-color: var(--rojo);
    color: var(--rojo);
  }
  main {
    overflow-y: auto;
    padding: 28px 36px;
  }
  .seccion {
    max-width: 760px;
    margin: 0 auto;
  }
  .seccion[hidden] {
    display: none;
  }
</style>
