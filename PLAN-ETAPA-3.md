# Plan Etapa 3 — Features post-0.4

Estado: EN CURSO (Fase 1)

El usuario eligió las 6 features de la lista. Se organizan en 5 fases, cada una
con release propio (así el auto-update se prueba en cada paso).

## Fase 1 — v0.5.0: Update en el arranque + Tiempo jugado
- **Banner de update en Home**: cuando `update_available` esté poblado, Home
  muestra un banner con "Hay una versión nueva: X" + botón *Actualizar a X* +
  ✕ para cerrar el aviso (solo esa sesión). Complementa el de Ajustes.
- **Tiempo jugado**: `Instance.playtime_secs: Option<u64>` (serde default, sin
  romper configs viejas). El hilo de lanzamiento manda `Playing{slug}` al
  arrancar el proceso y `PlaySession{slug, secs}` al salir; el handler suma y
  guarda el store. Home muestra "● En partida · N min" mientras corre.

## Fase 2 — Gestor de mods
- Vista en el detalle de instancia (solo Fabric/OptiFine/Forge/NeoForge):
  lista `mods/*.jar`, activar/desactivar (renombrar a `.disabled`), borrar,
  abrir carpeta. Sin descargas: eso ya lo hace Modrinth/drag&drop.

## Fase 3 — Galería de screenshots
- Pestaña o tarjeta en Home con `screenshots/*.png` de la instancia: miniaturas
  con texturas egui, clic = ver grande, botones abrir carpeta / copiar.

## Fase 4 — Copias de seguridad
- Exportar instancia → `.zip` (mundos, mods, options.txt, servers.dat) con
  Job + barra de progreso. Importar → restaura como instancia nueva.

## Fase 5 — Discord Rich Presence
- `discord-rich-presence` (IPC local, sin clave API): estado "Jugando
  Minecraft <versión> (<loader>)" + tiempo transcurrido. Toggle en Ajustes.
  Feature opcional en Cargo.toml para no engordar el exe base.
