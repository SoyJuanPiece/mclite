# Plan Etapa 3 — Features post-0.4

Estado: COMPLETA (Fases 1-5 en v0.5.0 + v0.7.0)

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

## Fase 2 — Gestor de mods ✔ (v0.7.0)
- `core/mods.rs`: lista `mods/*.jar` (+ `.disabled`), activar/desactivar
  (rename), borrar, con saneo de rutas. UI en Home (sección MODS, solo con
  cargador). Punto verde/gris = activo/apagado.

## Fase 3 — Galería de screenshots ✔ (v0.7.0)
- `core/shots.rs`: lista PNG por fecha, decodifica RGBA. Sección CAPTURAS en
  Home con miniaturas (texturas cacheadas por ruta), clic = visor grande con
  borrar, botón abrir carpeta.

## Fase 4 — Copias de seguridad ✔ (v0.7.0)
- `core/backup.rs`: export/import zip con manifiesto (`mclite-backup.json`).
  Incluye mundos (recursivo), mods (+.disabled), resourcepacks, shaders,
  screenshots, config, options.txt, servers.dat. Export con Job; import por
  drag&drop del .zip → instancia nueva + aviso de Reparar.

## Fase 5 — Discord Rich Presence ✔ (v0.7.0)
- `core/rpc.rs`: IPC local de Discord (pipe nominal Windows / unix socket),
  protocolo JSON con marco u32 LE, sin dependencias nuevas. Presencia al
  arrancar la partida ("Minecraft <versión> · con McLite" + timestamp), clear
  al salir. Toggle en Ajustes → Por defecto (on por defecto, no-op sin Discord).
