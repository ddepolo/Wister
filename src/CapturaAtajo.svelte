<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import { nombreAtajo, nombreTecla, teclaDeCodigo } from "./tipos";

  let { atajo, oncambiar }: { atajo: number[]; oncambiar: (teclas: number[]) => void } = $props();

  let capturando = $state(false);
  let apretadas = $state<number[]>([]);
  // La combinación más grande que se tuvo apretada: se toma al soltar todo.
  let combinacion = $state<number[]>([]);
  let aviso = $state("");

  function empezar() {
    capturando = true;
    apretadas = [];
    combinacion = [];
    aviso = "";
    // Si no, el atajo actual dispararía un dictado mientras se elige el nuevo.
    invoke("pausar_atajo", { pausado: true });
    window.addEventListener("keydown", abajo, true);
    window.addEventListener("keyup", arriba, true);
    window.addEventListener("blur", terminar);
  }

  function terminar() {
    if (!capturando) return;
    capturando = false;
    window.removeEventListener("keydown", abajo, true);
    window.removeEventListener("keyup", arriba, true);
    window.removeEventListener("blur", terminar);
    invoke("pausar_atajo", { pausado: false });
  }

  function abajo(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.repeat) return;
    if (e.code === "Escape") return terminar();
    const vk = teclaDeCodigo(e.code);
    if (vk === null) {
      aviso =
        e.key === "Meta"
          ? "La tecla Windows no se puede usar: abriría el menú Inicio."
          : "Usá Ctrl, Shift, Alt o teclas F (F1–F24).";
      return;
    }
    aviso = "";
    if (!apretadas.includes(vk)) apretadas = [...apretadas, vk];
    if (apretadas.length > combinacion.length) combinacion = [...apretadas];
  }

  function arriba(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    const vk = teclaDeCodigo(e.code);
    apretadas = apretadas.filter((t) => t !== vk);
    if (apretadas.length === 0 && combinacion.length > 0) {
      const nueva = combinacion.slice(0, 4);
      terminar();
      oncambiar(nueva);
    }
  }

  onDestroy(terminar);
</script>

<div class="atajo">
  {#if capturando}
    <span class="captura">
      {combinacion.length ? combinacion.map(nombreTecla).join(" + ") : "Apretá la combinación y soltá…"}
    </span>
    <button onclick={terminar}>Cancelar</button>
  {:else}
    <span class="actual">{nombreAtajo(atajo)}</span>
    <button onclick={empezar}>Cambiar</button>
  {/if}
</div>
{#if aviso}<p class="aviso">{aviso}</p>{/if}

<style>
  .atajo {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
  }
  .actual,
  .captura {
    flex: 1;
    padding: 4px 10px;
    border: 1px solid var(--borde);
    border-radius: 6px;
    font-size: 13px;
  }
  .captura {
    border-color: var(--acento);
    font-style: italic;
  }
  button {
    font: inherit;
    font-size: 13px;
    color: inherit;
    background: rgba(127, 127, 127, 0.12);
    border: 1px solid var(--borde);
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
  }
  .aviso {
    margin: 4px 0 0 90px;
    font-size: 12px;
    color: #d97706;
  }
</style>
