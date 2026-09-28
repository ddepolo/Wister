// Tipos que comparten las pantallas; reflejan los `Serialize` de src-tauri.

export type Estado =
  | { tipo: "iniciando" }
  | { tipo: "cargando_modelo"; modelo: string }
  | { tipo: "listo"; modelo: string; gpu: boolean }
  | { tipo: "grabando" }
  | { tipo: "transcribiendo" }
  | { tipo: "error"; mensaje: string };

export type Pegado =
  | { tipo: "hecho" | "ventana_elevada" | "teclas_apretadas" }
  | { tipo: "error"; mensaje: string };

/** Evento `resultado` de cada dictado (`dictado::Resultado`). */
export type Resultado =
  | {
      tipo: "texto";
      texto: string;
      audio_ms: number;
      espera_ms: number;
      microfono_ms: number;
      pegado: Pegado;
      app: string | null;
    }
  | { tipo: "descartado"; motivo: "otra_tecla" | "toque_corto" | "sin_voz" };

export const DESCARTES = {
  otra_tecla: "se tocó otra tecla",
  toque_corto: "toque muy corto",
  sin_voz: "no se detectó voz",
};

/** Por qué no se pegó el texto, o `null` si se pegó. */
export function noPegado(p: Pegado): string | null {
  switch (p.tipo) {
    case "hecho":
      return null;
    case "ventana_elevada":
      return "No se pudo pegar: la ventana corre como administrador. Quedó en el portapapeles.";
    case "teclas_apretadas":
      return "No se pegó porque seguían apretadas teclas modificadoras. Quedó en el portapapeles.";
    case "error":
      return `No se pudo pegar (${p.mensaje}).`;
  }
}

export type Config = {
  modelo: string | null;
  microfono: string | null;
  idioma: string;
  asistente_completo: boolean;
  mostrar_overlay: boolean;
  sonidos: boolean;
  atajo: number[];
  guardar_historial: boolean;
  /** Palabras del diccionario personal. */
  vocabulario: string[];
  /** Cambios sobre el texto transcripto; en `reemplazar`, `\n` es un salto de línea. */
  reemplazos: Reemplazo[];
};

export type Reemplazo = { buscar: string; reemplazar: string };

export type Modelo = {
  nombre: string;
  mb: number;
  nota: string;
  descargado: boolean;
  recomendado: boolean;
};

/** Un dictado guardado en el historial (`historial::Dictado`). */
export type Dictado = {
  id: number;
  /** Milisegundos desde 1970. */
  fecha: number;
  texto: string;
  audio_ms: number;
  palabras: number;
  /** Título de la ventana donde se pegó. */
  app: string | null;
};

export type CambioHistorial =
  | { tipo: "agregado"; dictado: Dictado }
  | { tipo: "borrado"; id: number }
  | { tipo: "borrado_todo" }
  | { tipo: "error"; mensaje: string };

export type Estadisticas = {
  palabras_hoy: number;
  palabras_semana: number;
  palabras_total: number;
  dictados_total: number;
  segundos_ahorrados: number;
  racha_dias: number;
  palabras_por_minuto: number;
};

export type Microfono = { nombre: string; predeterminado: boolean };

/** Volumen del micrófono en Windows (`volumen::Volumen`); `nivel` va de 0 a 1. */
export type Volumen = { nivel: number; silenciado: boolean };

/** Un modelo medido en la prueba de rendimiento (`rendimiento::Medicion`). */
export type Medicion = {
  modelo: string;
  gpu: boolean;
  carga_ms: number;
  transcripcion_ms: number;
  aciertos: number;
  texto: string;
  error: string | null;
};

/** Evento `rendimiento` (`rendimiento::Evento`). */
export type EventoRendimiento =
  | { tipo: "midiendo"; modelo: string; gpu: boolean; paso: number; total: number }
  | ({ tipo: "medicion" } & Medicion)
  | { tipo: "fin"; voz_ms: number; recomendado: string | null }
  | { tipo: "error"; mensaje: string };

export type Descarga =
  | { tipo: "progreso"; modelo: string; bajado: number; total: number | null }
  | { tipo: "lista"; modelo: string }
  | { tipo: "error"; modelo: string; mensaje: string };

export const IDIOMAS: [string, string][] = [
  ["es", "Español"],
  ["en", "Inglés"],
  ["pt", "Portugués"],
  ["fr", "Francés"],
  ["it", "Italiano"],
  ["de", "Alemán"],
  ["auto", "Varios idiomas (detectar automáticamente)"],
];

// Teclas que se pueden usar en un atajo (ver `hotkey::tecla_permitida` en Rust):
// `KeyboardEvent.code` del navegador → código virtual de Windows, con lado.
const MODIFICADORES: Record<string, [number, string]> = {
  ControlLeft: [0xa2, "Ctrl izq."],
  ControlRight: [0xa3, "Ctrl der."],
  ShiftLeft: [0xa0, "Shift izq."],
  ShiftRight: [0xa1, "Shift der."],
  AltLeft: [0xa4, "Alt izq."],
  AltRight: [0xa5, "Alt der. (AltGr)"],
};

/** Código virtual de Windows para una tecla del navegador, o `null` si no se permite. */
export function teclaDeCodigo(code: string): number | null {
  if (code in MODIFICADORES) return MODIFICADORES[code][0];
  const f = /^F(\d+)$/.exec(code);
  if (f && +f[1] >= 1 && +f[1] <= 24) return 0x70 + +f[1] - 1;
  return null;
}

export function nombreTecla(vk: number): string {
  const mod = Object.values(MODIFICADORES).find(([codigo]) => codigo === vk);
  if (mod) return mod[1];
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x70 + 1}`;
  return `0x${vk.toString(16)}`;
}

export const nombreAtajo = (teclas: number[]) => teclas.map(nombreTecla).join(" + ");
