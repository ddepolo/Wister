<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy, untrack } from "svelte";
  import {
    IDIOMAS,
    nombreAtajo,
    type Config,
    type Descarga,
    type Estado,
    type Microfono,
    type Modelo,
  } from "./tipos";

  let { config, onterminar }: { config: Config; onterminar: (c: Config) => void } = $props();

  const PASOS = ["Idioma", "Modelo", "Micrófono", "Probalo", "Listo"];

  let paso = $state(0);
  // Copia de trabajo: cada cambio se guarda en el momento (ver `aplicar`).
  let actual = $state<Config>(untrack(() => ({ ...config })));
  let modelos = $state<Modelo[]>([]);
  let microfonos = $state<Microfono[]>([]);
  let progreso = $state<Record<string, number | null>>({});
  let errorDescarga = $state("");
  let nivel = $state(0);
  let estado = $state<Estado>({ tipo: "iniciando" });
  let prueba = $state("");
  let autoarranque = $state(true);
  let error = $state("");

  const recomendado = $derived(modelos.find((m) => m.recomendado)?.nombre ?? "");
  const modeloElegido = $derived(actual.modelo ?? recomendado);
  const modeloListo = $derived(modelos.find((m) => m.nombre === modeloElegido)?.descargado ?? false);

  const listeners = [
    listen<Descarga>("descarga", ({ payload }) => {
      if (payload.tipo === "progreso") {
        progreso[payload.modelo] = payload.total ? (payload.bajado / payload.total) * 100 : null;
      } else if (payload.tipo === "lista") {
        delete progreso[payload.modelo];
        cargarModelos();
      } else {
        delete progreso[payload.modelo];
        errorDescarga = payload.mensaje;
      }
    }),
    listen<number>("nivel_prueba", ({ payload }) => (nivel = Math.min(1, Math.sqrt(payload / 0.12)))),
    listen<Estado>("estado", ({ payload }) => (estado = payload)),
  ];
  onDestroy(() => {
    listeners.forEach((l) => l.then((quitar) => quitar()));
    invoke("detener_prueba_microfono");
  });

  cargarModelos();
  invoke<Microfono[]>("listar_microfonos").then((m) => (microfonos = m));
  invoke<Estado>("estado_actual").then((e) => (estado = e));

  function cargarModelos() {
    invoke<Modelo[]>("listar_modelos").then((m) => (modelos = m));
  }

  /// Cada elección se guarda en el momento, así el paso "Probalo" ya dicta con ella.
  async function aplicar(cambios: Partial<Config>) {
    actual = { ...actual, ...cambios };
    try {
      await invoke("guardar_config", { config: actual });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  function descargar(nombre: string) {
    errorDescarga = "";
    progreso[nombre] = 0;
    invoke("descargar_modelo", { nombre }).catch((e) => {
      delete progreso[nombre];
      errorDescarga = String(e);
    });
  }

  function ir(nuevo: number) {
    if (paso === 2) invoke("detener_prueba_microfono");
    if (nuevo === 0 || nuevo === 1) aplicar({});
    if (nuevo === 2) invoke("probar_microfono", { nombre: actual.microfono });
    paso = nuevo;
  }

  function elegirMicrofono(nombre: string | null) {
    aplicar({ microfono: nombre });
    nivel = 0;
    invoke("probar_microfono", { nombre });
  }

  async function terminar() {
    try {
      await invoke("cambiar_autoarranque", { activo: autoarranque });
    } catch (e) {
      // No es grave: se puede activar después desde la configuración.
      console.error(e);
    }
    await aplicar({ asistente_completo: true });
    if (!error) onterminar(actual);
  }

  const puedeSeguir = $derived(paso !== 1 || modeloListo);
</script>

<div class="asistente">
  <ol class="pasos">
    {#each PASOS as nombre, i}
      <li class:hecho={i < paso} class:actual={i === paso}>{nombre}</li>
    {/each}
  </ol>

  <div class="contenido">
    {#if paso === 0}
      <h1>¡Bienvenido a Wister!</h1>
      <p>Dictado por voz que funciona en tu PC, sin nube. Primero, un par de preguntas.</p>
      <h2>¿En qué idioma vas a dictar?</h2>
      <div class="opciones">
        {#each IDIOMAS as [codigo, nombre]}
          <label class="opcion" class:elegida={actual.idioma === codigo}>
            <input
              type="radio"
              name="idioma"
              checked={actual.idioma === codigo}
              onchange={() => aplicar({ idioma: codigo })}
            />
            {nombre}
          </label>
        {/each}
      </div>
      {#if actual.idioma === "auto"}
        <p class="nota">
          Con frases muy cortas, la detección automática a veces confunde idiomas parecidos (por
          ejemplo, español y portugués). Si dictás casi siempre en uno, conviene elegirlo.
        </p>
      {/if}
    {:else if paso === 1}
      <h1>Modelo de voz</h1>
      <p>
        Wister transcribe con Whisper, que necesita un modelo descargado. Se baja una sola vez y
        después todo funciona sin internet.
      </p>
      <div class="opciones">
        {#each modelos as m}
          <label class="opcion modelo" class:elegida={m.nombre === modeloElegido}>
            <input
              type="radio"
              name="modelo"
              checked={m.nombre === modeloElegido}
              onchange={() => aplicar({ modelo: m.nombre })}
            />
            <span>
              <strong>{m.nombre}</strong>
              {#if m.recomendado}<span class="insignia">Recomendado</span>{/if}
              <span class="nota">{m.nota}</span>
              {#if m.aviso_ram}<span class="nota aviso-ram">{m.aviso_ram}</span>{/if}
            </span>
            <span class="tam">{m.descargado ? "Descargado ✓" : `${m.mb} MB`}</span>
          </label>
        {/each}
      </div>
      {#if modeloElegido in progreso}
        <div class="progreso"><div style="width: {progreso[modeloElegido] ?? 0}%"></div></div>
        <p class="nota">Descargando {modeloElegido}…</p>
      {:else if !modeloListo}
        <button class="principal" onclick={() => descargar(modeloElegido)}>
          Descargar {modeloElegido}
        </button>
      {/if}
      {#if errorDescarga}<p class="error">{errorDescarga}</p>{/if}
    {:else if paso === 2}
      <h1>Micrófono</h1>
      <p>Hablá un poco y fijate que la barra se mueva.</p>
      <select
        value={actual.microfono ?? ""}
        onchange={(e) => elegirMicrofono(e.currentTarget.value || null)}
      >
        <option value="">Predeterminado de Windows</option>
        {#each microfonos as m}
          <option value={m.nombre}>{m.nombre}</option>
        {/each}
      </select>
      <div class="medidor"><div style="width: {nivel * 100}%"></div></div>
      <p class="nota">Si no se mueve, probá con otro micrófono o revisá que no esté silenciado.</p>
    {:else if paso === 3}
      <h1>Probalo</h1>
      <p>
        Hacé clic en el cuadro, mantené apretado <kbd>{nombreAtajo(actual.atajo)}</kbd>, decí
        algo y soltá. Después podés cambiar el atajo en la configuración.
      </p>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea bind:value={prueba} autofocus placeholder="Acá va a aparecer lo que digas…"></textarea>
      <p class="nota">
        {#if estado.tipo === "grabando"}
          ● Grabando…
        {:else if estado.tipo === "transcribiendo"}
          Transcribiendo…
        {:else if estado.tipo === "cargando_modelo"}
          Cargando el modelo, esperá un momento…
        {:else if estado.tipo === "error"}
          <span class="error">{estado.mensaje}</span>
        {:else if prueba.trim()}
          ¡Funciona! Así va a pegar el texto en cualquier aplicación.
        {:else}
          Si mientras tenés el atajo apretado tocás otra tecla, se cancela.
        {/if}
      </p>
    {:else}
      <h1>¡Listo!</h1>
      <p>
        Wister queda en la bandeja del sistema, al lado del reloj. Desde su ícono podés abrir la
        configuración cuando quieras.
      </p>
      <label class="casilla">
        <input type="checkbox" bind:checked={autoarranque} />
        Iniciar Wister con Windows
      </label>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  <div class="botones">
    {#if paso > 0}
      <button onclick={() => ir(paso - 1)}>Atrás</button>
    {/if}
    <span class="espacio"></span>
    {#if paso < PASOS.length - 1}
      <button class="principal" disabled={!puedeSeguir} onclick={() => ir(paso + 1)}>
        Siguiente
      </button>
    {:else}
      <button class="principal" onclick={terminar}>Empezar a usar Wister</button>
    {/if}
  </div>
</div>

<style>
  .asistente {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    box-sizing: border-box;
    max-width: 640px;
    margin: 0 auto;
    padding: 28px 24px;
  }
  .pasos {
    display: flex;
    gap: 6px;
    margin: 0 0 20px;
    padding: 0;
    list-style: none;
    font-size: 12px;
  }
  .pasos li {
    flex: 1;
    padding-top: 6px;
    border-top: 3px solid var(--borde);
    opacity: 0.6;
  }
  .pasos li.hecho {
    border-color: var(--acento);
  }
  .pasos li.actual {
    border-color: var(--acento);
    opacity: 1;
    font-weight: 600;
  }
  .contenido {
    flex: 1;
    font-size: 14px;
  }
  h1 {
    margin: 0 0 8px;
    font-size: 22px;
  }
  h2 {
    margin: 18px 0 8px;
    font-size: 15px;
  }
  .opciones {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 10px 0;
  }
  .opcion {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border: 1px solid var(--borde);
    border-radius: 8px;
    cursor: pointer;
  }
  .opcion.elegida {
    border-color: var(--acento);
  }
  .modelo span:first-of-type {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .modelo .tam {
    margin-left: auto;
    font-size: 12px;
    opacity: 0.7;
    white-space: nowrap;
  }
  .insignia {
    align-self: flex-start;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 11px;
    background: var(--acento);
    color: #fff;
  }
  .nota {
    font-size: 12px;
    opacity: 0.7;
  }
  .aviso-ram {
    color: var(--naranja);
    opacity: 1;
  }
  .error {
    color: #dc2626;
    font-size: 13px;
    opacity: 1;
  }
  .progreso,
  .medidor {
    height: 8px;
    margin: 14px 0 4px;
    border-radius: 4px;
    background: rgba(127, 127, 127, 0.2);
    overflow: hidden;
  }
  .progreso div,
  .medidor div {
    height: 100%;
    background: var(--acento);
  }
  .progreso div {
    transition: width 150ms linear;
  }
  .medidor div {
    transition: width 60ms linear;
  }
  select,
  textarea {
    width: 100%;
    box-sizing: border-box;
    font: inherit;
    font-size: 14px;
    color: inherit;
    background: rgba(127, 127, 127, 0.1);
    border: 1px solid var(--borde);
    border-radius: 6px;
    padding: 8px 10px;
  }
  select option {
    color: #1d1d1f;
  }
  textarea {
    height: 120px;
    margin-top: 6px;
    resize: none;
  }
  kbd {
    padding: 1px 6px;
    border: 1px solid currentColor;
    border-radius: 4px;
    font-size: 12px;
    opacity: 0.8;
  }
  .casilla {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
  }
  .botones {
    display: flex;
    gap: 10px;
    padding-top: 16px;
  }
  .espacio {
    flex: 1;
  }
  button {
    font: inherit;
    font-size: 14px;
    color: inherit;
    padding: 7px 16px;
    border-radius: 6px;
    border: 1px solid var(--borde);
    background: rgba(127, 127, 127, 0.12);
    cursor: pointer;
  }
  button.principal {
    background: var(--acento);
    border-color: var(--acento);
    color: #fff;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
