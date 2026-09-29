<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Config, Reemplazo } from "./tipos";

  let {
    config,
    cambiar,
  }: { config: Config; cambiar: (cambios: Partial<Config>) => Promise<void> } = $props();

  /** Igual que `diccionario::MAXIMO_CARACTERES`: lo que pasa de ahí, Whisper no lo usa. */
  const MAXIMO_CARACTERES = 600;

  let nueva = $state("");
  let buscar = $state("");
  let reemplazarPor = $state("");
  let error = $state("");

  const largo = $derived(config.vocabulario.reduce((n, p) => n + p.length + 2, 0));

  async function guardar(vocabulario: string[]) {
    try {
      await cambiar({ vocabulario });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  /** Agrega lo escrito; con comas o saltos de línea se pueden pegar varias a la vez. */
  function agregar(e: SubmitEvent) {
    e.preventDefault();
    const vocabulario = [...config.vocabulario];
    for (const p of nueva.split(/[,;\n]/).map((p) => p.trim())) {
      if (p && !vocabulario.some((q) => q.toLowerCase() === p.toLowerCase())) vocabulario.push(p);
    }
    nueva = "";
    guardar(vocabulario);
  }

  const quitar = (palabra: string) => guardar(config.vocabulario.filter((p) => p !== palabra));

  async function guardarReemplazos(reemplazos: Reemplazo[]) {
    try {
      await cambiar({ reemplazos });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  function agregarReemplazo(e: SubmitEvent) {
    e.preventDefault();
    const b = buscar.trim();
    // Uno nuevo para la misma frase pisa al anterior.
    const otros = config.reemplazos.filter((r) => r.buscar.toLowerCase() !== b.toLowerCase());
    guardarReemplazos([...otros, { buscar: b, reemplazar: reemplazarPor }]);
    buscar = "";
    reemplazarPor = "";
  }

  /** Suma los comandos de voz que falten, sin tocar los que ya estén. */
  async function agregarComandos() {
    try {
      const comandos = await invoke<Reemplazo[]>("comandos_de_voz");
      const nuevos = comandos.filter(
        (c) => !config.reemplazos.some((r) => r.buscar.toLowerCase() === c.buscar.toLowerCase()),
      );
      guardarReemplazos([...config.reemplazos, ...nuevos]);
    } catch (e) {
      error = String(e);
    }
  }

  const quitarReemplazo = (i: number) =>
    guardarReemplazos(config.reemplazos.filter((_, j) => j !== i));

  /** Cómo se muestra el reemplazo: los saltos de línea y el borrado, con palabras. */
  const mostrar = (r: string) =>
    r === "" ? "(se borra)" : r.replaceAll("\\n", "⏎ salto de línea");
</script>

<h1>Diccionario</h1>
{#if error}<p class="error">{error}</p>{/if}

<section class="tarjeta">
  <h2>Vocabulario</h2>
  <p class="muted">
    Palabras que Whisper no conoce o escribe mal: nombres propios, marcas, siglas, jerga de tu
    trabajo. Se las pasamos como pista en cada dictado para que las escriba así cuando suenen
    parecido. Ayuda mucho, pero no es una garantía.
  </p>
  <form onsubmit={agregar}>
    <input
      type="text"
      placeholder="Por ejemplo: Wister, Kubernetes, Martina Pérez"
      bind:value={nueva}
    />
    <button type="submit" disabled={!nueva.trim()}>Agregar</button>
  </form>

  {#if config.vocabulario.length}
    <ul>
      {#each config.vocabulario as palabra}
        <li>
          {palabra}
          <button class="quitar" title="Quitar" aria-label="Quitar {palabra}" onclick={() => quitar(palabra)}>
            ×
          </button>
        </li>
      {/each}
    </ul>
    {#if largo > MAXIMO_CARACTERES}
      <p class="aviso">
        Hay más palabras de las que Whisper puede usar como pista: se usan las primeras. Sacá las que
        ya escribe bien.
      </p>
    {/if}
  {:else}
    <p class="muted vacio">Todavía no agregaste palabras.</p>
  {/if}
</section>

<section class="tarjeta">
  <h2>Reemplazos</h2>
  <p class="muted">
    Cambian el texto después de transcribirlo, así que se aplican siempre: sirven cuando Whisper
    insiste en escribir algo de otra forma ("Chat GPT" → "ChatGPT") y para comandos como "punto y
    aparte". Se buscan palabras o frases completas, sin distinguir mayúsculas.
  </p>
  <form class="reemplazo" onsubmit={agregarReemplazo}>
    <input type="text" placeholder="Si escribe…" aria-label="Buscar" bind:value={buscar} />
    <span class="flecha">→</span>
    <input
      type="text"
      placeholder="…poner esto"
      aria-label="Reemplazar por"
      bind:value={reemplazarPor}
    />
    <button type="submit" disabled={!buscar.trim()}>Agregar</button>
  </form>
  <p class="muted ayuda">
    Dejá el segundo vacío para borrar la frase (sirve para muletillas) y escribí <code>\n</code>
    para un salto de línea.
  </p>

  {#if config.reemplazos.length}
    <table>
      <tbody>
        {#each config.reemplazos as r, i}
          <tr>
            <td>{r.buscar}</td>
            <td class="flecha">→</td>
            <td class:borrado={r.reemplazar === ""}>{mostrar(r.reemplazar)}</td>
            <td class="accion">
              <button
                class="quitar"
                title="Quitar"
                aria-label="Quitar el reemplazo de {r.buscar}"
                onclick={() => quitarReemplazo(i)}>×</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="muted vacio">Todavía no agregaste reemplazos.</p>
  {/if}
  <div class="comandos">
    <button onclick={agregarComandos}>Agregar comandos de voz</button>
    <span class="muted">
      "punto y aparte", "nueva línea", "punto y coma", "dos puntos", "abrir paréntesis" y "cerrar
      paréntesis". Podés quitar los que no uses.
    </span>
  </div>
</section>

<style>
  .tarjeta {
    margin-top: 16px;
  }
  p {
    margin: 0 0 10px;
    font-size: 13px;
    line-height: 1.5;
  }
  form {
    display: flex;
    gap: 8px;
  }
  input[type="text"] {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 13px;
    color: inherit;
    background: var(--suave);
    border: 1px solid var(--borde);
    border-radius: 6px;
    padding: 4px 10px;
  }
  input[type="text"]:focus {
    outline: none;
    border-color: var(--acento);
  }
  ul {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px 4px 2px 10px;
    border: 1px solid var(--borde);
    border-radius: 999px;
    font-size: 13px;
  }
  .quitar {
    padding: 0 6px;
    border: none;
    background: none;
    font-size: 15px;
    line-height: 1;
    opacity: 0.6;
  }
  .quitar:hover {
    opacity: 1;
  }
  .vacio {
    margin: 12px 0 0;
  }
  .aviso {
    margin: 10px 0 0;
    color: var(--naranja);
  }
  .reemplazo {
    align-items: center;
  }
  .comandos {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
    font-size: 12px;
  }
  .comandos button {
    flex: none;
  }
  .flecha {
    opacity: 0.6;
  }
  .ayuda {
    margin: 6px 0 0;
    font-size: 12px;
  }
  code {
    font-size: 12px;
  }
  table {
    width: 100%;
    margin-top: 12px;
    border-collapse: collapse;
    font-size: 13px;
  }
  td {
    padding: 4px 6px;
    border-bottom: 1px solid var(--borde);
  }
  td.flecha {
    width: 16px;
    text-align: center;
  }
  td.borrado {
    font-style: italic;
    opacity: 0.7;
  }
  td.accion {
    width: 28px;
    text-align: right;
  }
</style>
