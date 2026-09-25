# McLite — Plan Etapa 2: auto-update del launcher y skins offline

> Cierra las dos carencias más visibles después del plan de UI (`PLAN-UI.md`, completado en v0.3.0).
> Dos features con una particularidad técnica cada una: el exe **no puede sobrescribirse a sí mismo**
> mientras corre (auto-update), y el juego vanilla **no carga skins de cuentas offline** sin un mod
> (skins). Este plan dice cómo se resuelven ambas de forma honesta.

---

## A. Auto-update del launcher

### A.0 Estado actual (v0.3.0)
Ya hay aviso: consulta `api.github.com/repos/SoyJuanPiece/mclite/releases/latest` al arrancar,
toast con botón "Descargar" que abre el release en el navegador. Falta el paso final: que el
launcher se actualice él solo.

### A.1 El problema del self-replace en Windows
Un `.exe` en ejecución está bloqueado para escritura/borrado. La solución estándar (la que usan
Chromium, VS Code, etc.) aprovecha que **renombrar un exe en ejecución SÍ está permitido**:

1. Bajar `mclite.exe` nuevo a `mclite/mclite/update/mclite.exe.new` (carpeta de datos, siempre escribible).
2. Verificar SHA-256 contra el fichero `.sha256` subido junto al asset.
3. `rename("mclite.exe" → "mclite.exe.old")` (permitido en ejecución; si un antivirus lo bloquea, fallback al paso 5).
4. Copiar el nuevo a `mclite.exe`. La próxima vez que el usuario abra el launcher, arranca el nuevo.
5. Helper de limpieza: un mini `.bat` (o `cmd /C` con `timeout`) que espera a que el proceso salga,
   borra `mclite.exe.old` y relanza el launcher nuevo. Fallback si el rename falló: el helper hace
   también el `copy /Y` después de la salida.
6. Al arrancar: si existe `mclite.exe.old`, intentar borrarlo (si está bloqueado, se reintenta en el
   próximo arranque — nunca es fatal).

Bonus: el binario descargado por el propio launcher **no lleva Mark of the Web**, así que no
dispara SmartScreen (a diferencia de bajarlo con el navegador).

### A.2 Infraestructura de releases
- `scripts/build-windows.sh` genera también `mclite.exe.sha256` (hex + nombre) junto al exe.
- Cada release sube **dos assets**: `mclite.exe` y `mclite.exe.sha256`. El updater los localiza por
  `assets[].browser_download_url` del JSON de `latest`.
- `sha2` es crate nuevo (deps actuales: sha1, md-5). Coste ~irrelevante en tamaño.

### A.3 Flujo de usuario
- **Toast** "Nueva versión X disponible" con dos botones: *Actualizar* (en-app) y *Ver release*.
- **Ajustes → ACTUALIZACIONES**: estado ("Estás en 0.3.0, hay 0.4.0"), botón "Actualizar ahora",
  toggle de comprobación automática (ya existe).
- Durante la descarga: `job` normal con barra y ETA ("Actualizando McLite").
- Al verificar: modal simple "Reiniciar ahora" / "La próxima vez". El swap ocurre igualmente;
  el reinicio solo lo aplica al momento.

### A.4 Core
`core/updater.rs`: `latest_release(http) -> Option<ReleaseInfo{version, exe_url, sha256_url}>`,
`download_update(paths, release, progress) -> Result<PathBuf>` (a `update/`),
`verify(sha256)`, `apply_swap(exe_dir, new_exe) -> SwapOutcome` (rename+copy+helper), test del
comparador de versiones ya existente (`version_is_newer`) y de `verify` con hash válido/inválido.

### A.5 Riesgos
| Riesgo | Mitigación |
|---|---|
| Antivirus bloquea el rename en ejecución | Fallback: helper `.bat` hace todo tras la salida |
| Usuario apaga el launcher a mitad del swap | El `.old` queda; el nuevo exe ya está en su sitio; arranque limpia |
| Corrupción de descarga | SHA-256 obligatorio antes de tocar nada; si falla, toast de error y no se toca el exe |
| Release sin `.sha256` (releases antiguos) | El updater ignora releases sin el par de assets y avisa "actualiza a mano" |

**Criterios de aceptación**
- [ ] De 0.3.0 → 0.4.0: toast, descarga con barra, verify, swap, relanzar = título "0.4.0", config e instancias intactas.
- [ ] `mclite.exe.old` desaparece tras el primer arranque posterior.
- [ ] Descarga corrupta (hash mal) → error, exe original intacto.

---

## B. Skins para cuentas offline

### B.0 La verdad incómoda (y por qué este plan es el que es)
El juego pide la textura de la skin al **servidor de sesiones de Mojang** usando el UUID del perfil.
Una cuenta offline tiene un UUID sintético **sin texturas** → el juego pinta Steve/Alex por defecto.
Ningún launcher puede cambiar eso *sin un mod* en vanilla offline: no hay parámetro de lanzamiento
`--skin`. (Los launchers que "sí lo hacen" instalan un mod por debajo, o interceptan authlib.)

Lo que sí funciona bien, sin hacks: **CustomSkinLoader (CSL)**, mod abierto que carga skins
locales/URL y se lleva bien con offline + singleplayer + servidores `online-mode=false`.

### B.1 Alcance
1. **Núcleo de skins** (`core/skins.rs`):
   - `from_local_png(path)`: el usuario arrastra/suelta (o elige con diálogo) su skin 64×32/64×64.
   - `from_nick_premium(http, nick)`: si el nick pertenece a un jugador premium, bajar **su** skin
     (`api.mojang.com/users/profiles/minecraft/<nick>` → UUID → `sessionserver.mojang.com/.../textures`
     → decodificar el base64 → URL de la textura). Es lo que hace SkinsRestorer: "tu nick premium, tu skin".
   - Guardar en `cache/skins/<nick>.png` (misma idea que la caché de iconos).
   - Aplicar a una instancia = copiar a `<gameDir>/CustomSkinLoader/LocalSkin/<nick>.png`
     (verificar el layout exacto de CSL en implementación: `LocalSkin/<nick>.png` + capas en `LocalSkinCapes/`).
2. **Instalador de CSL por instancia**: igual que el botón de Sodium — botón "Instalar soporte de skins"
   para instancias **Fabric y Forge** (resolver la versión correcta del mod desde Modrinth; si no está
   en Modrinth, cfwidget/CurseForge sin key — *verificar* en implementación; el fallback honesto es
   pedir el jar con drag & drop).
3. **Vista previa en Ajustes**: recortar la cabeza (8×8 en `8..16,8..16`) y pintarla escalada con
   `egui::ColorImage` + `ctx.load_texture` — previsualizar el nick real de la cuenta. Requiere crate
   `image` con feature `png` (ya viene transitivamente por egui_extras; hacerlo directo).
4. **Diálogo de fichero**: crate `rfd` (nativo, ligero) para "Elegir PNG…" — o solo drag & drop para
   no añadir deps (decisión en implementación; el drop ya existe de la Fase 4).

### B.2 UX
- **Ajustes → CUENTA**: preview de la cabeza + nick; "Cambiar skin" (PNG local), "Usar la skin de mi nick (premium)", "Quitar".
- **Home** (instancia seleccionada): botón "Skins" si el cargador lo soporta → instala CSL si falta y aplica la skin actual.
- **Toasts** con el resultado ("Skin aplicada a «Zombie Invade»", "Ese nick no es premium, sin skin remota").
- **Honesto en la UI**: si la instancia es Vanilla → nota "vanilla offline no soporta skins; usa Fabric/Forge".

### B.3 Riesgos
| Riesgo | Mitigación |
|---|---|
| CSL no está en Modrinth para la versión pedida | Fallback cfwidget/CurseForge (sin key) o drag & drop del jar; *verificar* endpoints |
| Layout de `LocalSkin` cambia entre versiones de CSL | Fijar la versión de CSL probada por instancia; test con un jar real |
| El nick premium existe pero sin skin | Mensaje claro; queda la opción PNG local |
| Server con `online-mode=false` y plugin de skins propio | CSL convive; el plugin manda en remotos, CSL en locales — documentado en README |

**Criterios de aceptación**
- [ ] Fabric + CSL + PNG arrastrado = skin visible en singleplayer offline (la prueba definitiva).
- [ ] Nick premium → su skin real aplicada offline.
- [ ] Ajustes muestra la cabeza de la skin actual junto al nick.
- [ ] "Instalar soporte de skins" deja CSL en `mods/` y se reinstala en Reparar sin perder la skin.

---

## C. Orden de trabajo

| Fase | Entregable | Riesgo |
|---|---|---|
| **A1** | Release con `.sha256` + `core/updater.rs` (check/download/verify) + tests | bajo |
| **A2** | Swap en ejecución + helper de limpieza + UI (toast y Ajustes) | **medio** (el único movimiento delicado del plan) |
| **B1** | `core/skins.rs` (PNG local + nick premium) + vista previa en Ajustes | bajo |
| **B2** | Instalador CSL por instancia + aplicación a `LocalSkin` + pruebas reales en juego | medio (depende de endpoints a verificar) |

Fuera de alcance: subida de skins a Mojang (requiere cuenta premium), skins para Vanilla sin mods,
capas propias del launcher (CSL ya las soporta; se añade si sobra tiempo).

## Criterio global de "listo"
- [ ] Auto-update: de la 0.3.0 a la siguiente release, todo en-app, sin navegador.
- [ ] Skins: PNG local o nick premium, aplicado y visible en el juego real (no solo en la UI).
- [ ] `cargo test` verde, exe < 10,5 MB, release etiquetado por fase (v0.4.x / v0.5.x).
