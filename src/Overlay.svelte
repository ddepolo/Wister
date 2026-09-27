<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

  // Mitad de las barras: se dibujan en espejo, con lo más reciente en el centro.
  const HISTORIA = 14;

  let modo = $state<"grabando" | "transcribiendo">("grabando");
  let historia = $state<number[]>(Array(HISTORIA).fill(0));
  const barras = $derived([...historia].reverse().concat(historia));

  listen<{ tipo: string }>("estado", ({ payload }) => {
    if (payload.tipo === "grabando") {
      modo = "grabando";
      historia = Array(HISTORIA).fill(0);
    } else if (payload.tipo === "transcribiendo") {
      modo = "transcribiendo";
    }
  });

  // RMS de cada tramo de 50 ms. La voz normal da entre 0,02 y 0,2: la raíz
  // comprime el rango para que se vea movimiento también hablando bajo.
  listen<number>("nivel", ({ payload }) => {
    const alto = Math.min(1, Math.sqrt(payload / 0.12));
    historia = [alto, ...historia.slice(0, HISTORIA - 1)];
  });
</script>

<div class="pastilla">
  {#if modo === "grabando"}
    <div class="onda">
      {#each barras as n}
        <!-- En silencio cada barra es un cuadradito del mismo ancho que la barra. -->
        <span style="height: max(3px, {n * 100}%)"></span>
      {/each}
    </div>
  {:else}
    <div class="pensando"><span></span><span></span><span></span></div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  :global(#app) {
    height: 100%;
    display: grid;
    place-items: center;
  }
  .pastilla {
    box-sizing: border-box;
    width: 184px;
    height: 40px;
    padding: 0 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: rgba(24, 24, 28, 0.88);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }
  .onda {
    flex: 1;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .onda span {
    flex: none;
    width: 3px;
    background: #f4f4f5;
    transition: height 60ms linear;
  }
  .pensando {
    display: flex;
    gap: 6px;
  }
  .pensando span {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #a5b4fc;
    animation: salto 0.9s ease-in-out infinite;
  }
  .pensando span:nth-child(2) {
    animation-delay: 0.15s;
  }
  .pensando span:nth-child(3) {
    animation-delay: 0.3s;
  }
  @keyframes salto {
    0%,
    100% {
      transform: translateY(0);
      opacity: 0.5;
    }
    50% {
      transform: translateY(-5px);
      opacity: 1;
    }
  }
</style>
