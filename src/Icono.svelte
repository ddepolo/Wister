<script lang="ts">
  let {
    nombre,
  }: { nombre: "inicio" | "historial" | "diccionario" | "configuracion" | "salir" } = $props();
</script>

<svg
  viewBox="0 0 24 24"
  width="18"
  height="18"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if nombre === "inicio"}
    <path d="M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z" />
  {:else if nombre === "historial"}
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  {:else if nombre === "diccionario"}
    <path d="M5 5a2 2 0 0 1 2-2h12v15H7a2 2 0 0 0-2 2z" />
    <path d="M5 20a1 1 0 0 0 1 1h13v-3" />
  {:else if nombre === "salir"}
    <path d="M12 3v8" />
    <path d="M6.3 6.3a8 8 0 1 0 11.4 0" />
  {:else}
    <path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1" />
    <circle cx="15" cy="6" r="2" />
    <circle cx="9" cy="12" r="2" />
    <circle cx="17" cy="18" r="2" />
  {/if}
</svg>
