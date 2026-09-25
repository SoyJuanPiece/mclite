# McLite — Plan de pulido de UI ("nivel pro")

> Objetivo: que el launcher se sienta **fluido, coherente y bonito** en todas sus pantallas,
> con coste de rendimiento ~cero en reposo. Basado en auditoría real del código (09/2026).
> Complementa a `PLAN.md` (arquitectura). Los fases son independientes: cada una deja la app mejorable por separado.

## 0. Dónde estamos (auditoría)

**Ya bien** (no tocar): tema verde con acento dinámico (5 presets), Inter + SemiBold, esquinas
8px globales, avatar pintado a mano, logo de hierba, toasts con auto-cierre, transición de
pantalla, barra de progreso propia con ETA, sidebar agrupado (Tus instancias / Modpacks),
banner con tooltip, iconos de Modrinth en sidebar y Home.

**Audiencia del problema** — dónde se nota "feo" o "lento" hoy:

| Área | Síntoma | Causa en código |
|---|---|---|
| Rendimiento | CPU despierta siempre (batería, ventilador) | `update()` termina con `request_repaint_after(120ms)`: la app repinta a 8 Hz **para siempre**, incluso sin nada que hacer |
| Rendimiento | Descargas "a saltos" | Cada evento de progreso → mensaje → repintado completo; sin agrupar |
| Consistencia | "Nueva instancia" y "Editar" se ven planas/grises | 0 usos de `theme::card()`; secciones como texto suelto; listas `selectable_label` sin contenedor |
| Consistencia | Modpacks: tarjetas irregulares | Filas `horizontal` sin tamaño de tarjeta unificado |
| Detalle | Toasts: solo se ve el último | `show_toasts` dibuja `toasts.last()` |
| Detalle | Transición de pantalla hecha a mano | `screen_fade` manual; egui trae `animate_value_with_time` para esto |
| UX | Todo con el ratón | Sin atajos, sin menú contextual, sin doble clic |
| UX | Iconos se re-descargan cada sesión | egui_extras cachea solo en memoria |
| Onboarding | Primer arranque vacío | Solo "Crear una instancia"; nick/acento quedan para después |

---

## Fase 1 — Rendimiento (el "pro" invisible) · ~1 sesión

La que más se siente aunque no se vea. **Meta: 0 % de CPU en reposo y descargas suaves.**

1. **Repintado dirigido por eventos.** Guardar `egui::Context` en `GuiSink`; cada mensaje que
   llega llama `ctx.request_repaint()`. Quitar el `request_repaint_after(120ms)` fijo y solo
   mantenerlo mientras haya `job` activo (o eliminarlo del todo si los eventos ya despiertan).
   - Resultado: app abierta sin hacer nada = 0 % CPU (hoy: repinta 8 veces/seg).
2. **Coalescer progreso.** En `GuiSink`, acumular `Advance` y enviar como máximo cada ~33 ms
   (o cada N unidades); `Phase`/`Message` pasan inmediatos. Con (1), esto evita 200 repintados/s
   durante una descarga y deja la barra fluida a ~30 fps.
3. **Animaciones con egui, no a mano.** Cambiar `screen_fade`/alpha de toasts a
   `ctx.animate_value_with_time(id, target, 0.18)`: frames solo mientras anima, easing propio,
   y menos estado en `McLiteApp` (adiós `screen_from`).
4. **Medir.** Log de diagnóstico opt-in (Ajustes → "Mostrar FPS/CPU en la barra") usando
   `ctx.input(|i| i.time)`… solo mientras esté activado.

**Criterios:** en reposo 60 s → 0 repintados (verificable con el contador de frames); durante
una descarga la barra avanza fluida y la CPU del launcher se mantiene < 2 % en un i5.

## Fase 2 — Consistencia visual en TODAS las pantallas · ~1 sesión

Que "Nueva instancia", "Editar" y "Modpacks" luzcan como Home/Ajustes.

1. **Sistema de tarjetas:** helper `theme::card_section(ui, "RÓTULO", |ui| …)` (card + section
   header dentro, como la `card()` de settings) y usarlo en las 3 pantallas que quedan planas.
2. **Formularios alineados:** helper `theme::form_row(ui, "Etiqueta", |ui| control)` — etiqueta
   a ancho fijo, control alineado; usar en RAM, resolución, Java, nick.
3. **Listas de versiones con estilo:** las `selectable_label` de versiones de MC/cargador dentro
   de una tarjeta con scroll propio, filas de altura fija y hover animado (ya lo da
   `animation_time`), badge "latest" en la fila de la versión más reciente.
4. **Control segmentado con contenedor:** pills dentro de una pista redondeada (fondo INPUT),
   seleccionado = acento. `widgets::segmented` ya existe: solo envolverlo.
5. **Modpacks en rejilla uniforme:** tarjetas de ancho fijo (~230px) con `egui::Grid`/wrap,
   icono cuadrado 96px arriba, título a 2 líneas máximo, descargas formateadas ("12,3 k"),
   borde de acento al hover. Detalle del pack: cabecera tipo banner como Home.
6. **Botones con jerarquía:** primario (acento) / secundario (ghost) / peligro (Borrar en rojo
   suave) — helpers ya existen; aplicarlos en todos los flujos (Crear, Guardar, Cancelar).

**Criterios:** capturas de las 5 pantallas a 1280×720 sin elementos desalineados; un solo
estilo de tarjeta/botón en toda la app.

## Fase 3 — Pulido y detalle · ~1 sesión

1. **Toasts apilables** (máx. 3, con deslizamiento de entrada animado) y **toast de progreso**
   opcional cuando la app está en otra pantalla.
2. **Estados vacíos bonitos:** Modpacks sin resultados (logo + "prueba con otra búsqueda"),
   primera apertura de cada pantalla.
3. **Micro-interacciones:** hover con leve elevación en tarjetas (borde + fill ya existe;
   añadir desplazamiento de 1px), focus visible con anillo de acento en inputs.
4. **Caché de iconos en disco:** `cache/icons/<sha1(url)>` + prefiero por hilo de fondo;
   `Image::from_uri("file://…")` cuando exista. Modpacks abre instantáneo la 2ª vez y offline.
5. **Tipografía:** Inter Medium para valores de formulario (RAM "4096 MB") y Display para el
   título del banner (+0,4 MB; evaluar si el exe sigue < 10 MB).
6. **Barra de estado:** reusar `widgets::progress` (misma barra que Home, versión mini) y
   punto de estado (verde=ok, rojo=error) en vez de texto plano.

**Criterios:** segunda visita a Modpacks sin red muestra iconos; ningún texto se corta a
ancho mínimo de ventana (820px).

## Fase 4 — UX de launcher pro · ~1–2 sesiones

1. **Menú contextual en el sidebar** (clic derecho en instancia): Jugar · Editar · Reparar ·
   Carpeta · Borrar. Doble clic = JUGAR.
2. **Atajos:** Ctrl+Enter Jugar, Ctrl+N nueva instancia, Ctrl+F buscar, Esc volver.
3. **Drag & drop definitivo:** soltar `.mrpack` en la ventana → flujo de instalación; soltar
   `.jar` sobre una instancia Fabric → se copia a `mods/` con toast. (Era feature a medias.)
4. **Onboarding de 2 clics:** primer arranque → tarjeta de bienvenida: nick + color de acento
   + "Crear mi primera instancia". Guarda config y salta a Nueva instancia.
5. **Buscador de updates:** al arrancar, GET a `api.github.com/repos/SoyJuanPiece/mclite/releases/latest`
   (hilo de fondo, silencioso si falla); si hay versión nueva → toast con botón que abre el
   release. Respetar un toggle en Ajustes ("Buscar actualizaciones").
6. **Datos útiles en Home:** nº de mods de la instancia y tamaño de su carpeta (calculado en
   hilo de fondo al seleccionar), junto a "Última partida".

**Criterios:** todas las acciones de la instancia alcanzables sin ratón; soltar un `.mrpack`
instala sin tocar Modpacks; onboarding completo en < 30 s.

## Fuera de alcance (deliberado)

- **Barra de título propia (CSD):** rompe arrastre/redimensionado/Windows fácil; el riesgo no
  compensa. Revisar solo cuando egui lo estabilice.
- Fuentes por CDN, animaciones decorativas largas, dependencias GUI nuevas grandes
  (todo lo del plan usa egui 0.32 + crates ya presentes).
- Skins online, login Microsoft: fuera del proyecto (cuentas offline por diseño).

## Criterios globales de "listo"

- [ ] Reposo 60 s = 0 % CPU (Fase 1 verificada con contador de frames).
- [ ] Las 5 pantallas comparten tarjeta/rótulo/botones (auditoría visual lado a lado).
- [ ] Ningún desborde de texto a 820×520 (mínimo) ni a 1920×1080.
- [ ] Todo flujo tiene estado vacío, de carga y de error con toast.
- [ ] `cargo test --no-default-features` verde; exe < 10,5 MB.
- [ ] Cada fase = 1 commit + release etiquetado (v0.2.x).
