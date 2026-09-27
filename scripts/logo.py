"""Genera src-tauri/icons/icon.svg: una onda de voz con degradé cian → violeta que
termina en un punto brillante, sobre fondo azul noche.

Después: npx tauri icon src-tauri/icons/icon.svg -o src-tauri/icons
"""

C = 262  # altura de la línea base


def onda():
    x = 70
    d = [f"M {x} {C}"]
    # (ancho, amplitud) de cada media onda; negativo = hacia arriba.
    tramos = [(30, 0), (38, -30), (40, 58), (44, -120), (44, 124), (42, -84), (36, 36)]
    for w, a in tramos:
        # Una cúbica con los controles a 4/3 de la amplitud tiene su pico en la amplitud.
        h = a * 4 / 3
        d.append(f"C {x + w * 0.36:.1f} {C - h:.1f} {x + w * 0.64:.1f} {C - h:.1f} {x + w:.1f} {C}")
        x += w
    # Pico final, finito, como un latido antes del punto.
    d.append(f"L {x + 14} {C} L {x + 24} {C - 58} L {x + 36} {C + 26} L {x + 44} {C}")
    x += 44
    fin = x + 30
    d.append(f"L {fin} {C}")
    return " ".join(d), fin


camino, fin = onda()
svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <defs>
    <linearGradient id="fondo" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0c1f45"/>
      <stop offset="1" stop-color="#071329"/>
    </linearGradient>
    <radialGradient id="luz" cx="0.5" cy="0.45" r="0.6">
      <stop offset="0" stop-color="#1b4f7a" stop-opacity="0.55"/>
      <stop offset="1" stop-color="#1b4f7a" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="trazo" x1="70" y1="0" x2="{fin}" y2="0" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#22d3ee" stop-opacity="0.4"/>
      <stop offset="0.25" stop-color="#38bdf8"/>
      <stop offset="0.6" stop-color="#60a5fa"/>
      <stop offset="0.85" stop-color="#a78bfa"/>
      <stop offset="1" stop-color="#e9d5ff"/>
    </linearGradient>
    <radialGradient id="brillo">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset="0.35" stop-color="#e9d5ff" stop-opacity="0.9"/>
      <stop offset="1" stop-color="#a78bfa" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="512" height="512" rx="112" fill="url(#fondo)"/>
  <rect width="512" height="512" rx="112" fill="url(#luz)"/>
  <path d="{camino}" fill="none" stroke="url(#trazo)" stroke-width="64" stroke-linecap="round" stroke-linejoin="round" opacity="0.18"/>
  <path d="{camino}" fill="none" stroke="url(#trazo)" stroke-width="30" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="{fin}" cy="{C}" r="40" fill="url(#brillo)"/>
  <circle cx="{fin}" cy="{C}" r="14" fill="#ffffff"/>
</svg>
"""
with open("src-tauri/icons/icon.svg", "w", encoding="utf-8") as f:
    f.write(svg)
