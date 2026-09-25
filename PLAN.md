# McLite — plan para un launcher lite de Minecraft (Rust, nativo Windows)

> Estado: **plan / diseño**. Nada implementado todavía.
> Decisiones ya cerradas con el usuario: **Rust + egui/eframe**, **cuentas solo offline**, y un
> selector de cargador con **Vanilla / Fabric / Forge (+NeoForge) / OptiFine** y **toggle de snapshots**.

---

## 1. Resumen

Un solo `.exe` nativo de Windows (x86_64), sin .NET, sin Electron, sin Node, sin instalador.
El launcher:

1. Lee el manifiesto oficial de Mojang y lista versiones (oficiales por defecto, snapshots si el usuario activa el toggle).
2. Permite elegir **cargador + versión del cargador** (Vanilla, Fabric, Quilt, Forge, NeoForge, OptiFine).
3. Descarga client jar, librerías, natives y assets (en paralelo, con verificación SHA-1).
4. Genera el manifiesto final de la instancia (merge de vanilla + loader).
5. Detecta o **descarga el runtime de Java de Mojang** para que el usuario no instale nada.
6. Lanza con una cuenta offline (nickname local).
7. Muestra todo eso en una UI oscura, limpia y liviana.

Objetivo de calidad: `.exe` **< 15 MB**, arranque **< 1 s**, UI en reposo **< 150 MB RAM**.

**La restricción real del proyecto no es la UI, es el instalador de Forge.** Ordenar el trabajo
para llegar cuanto antes a "vanilla jugable" y dejar Forge para una fase con presupuesto propio.

---

## 2. Hallazgos de la investigación (verificado con requests reales)

Todo lo de abajo se comprobó el **2026-09-24** con requests reales desde este entorno. Lo que no
pude verificar está marcado explícitamente como **verificar**.

| Cosa | Endpoint / hallazgo | Estado |
|---|---|---|
| Manifiesto de versiones | `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` | ✅ estándar |
| Version JSON | URL en `versions[].url` del manifiesto | ✅ estándar |
| Runtime de Java (Mojang) | `https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json` | ✅ 200, trae `windows-x64` con `java-runtime-alpha/beta/delta/gamma` y `jre-legacy` |
| Fabric: versiones de loader | `https://meta.fabricmc.net/v2/versions/loader/{mc}` | ✅ 200 (trae `loader.version`, `stable`, `launcherMeta.libraries`) |
| Fabric: version json listo | `https://meta.fabricmc.net/v2/versions/loader/{mc}/{loader}/profile/json` | ✅ documentado, **verificar** el body |
| Quilt | `https://meta.quiltmc.org/v3/versions/loader/{mc}` + `/…/{loader}/profile/json` | verificar |
| Forge: lista de versiones | `https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml` | ✅ 200, versiones `{mc}-{forge}` (ej. `1.21.4-54.1.6`) |
| Forge: instalador | `https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{forge}/forge-{mc}-{forge}-installer.jar` | ✅ 206 `application/java-archive` |
| Forge: `promotions_slim.json` | `files.minecraftforge.net/…/promotions_slim.json` | ❌ **devuelve 403 a clientes automatizados** desde ~2024 → **no depender de él** |
| NeoForge | `https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml` — versiones `{21.4}.x` para MC `1.21.4` | verificar |
| **OptiFine: lista** | `https://bmclapi2.bangbang93.com/optifine/{mc}` | ✅ 200, devuelve `{type:"HD_U", patch:"J3", filename:"OptiFine_1.21.4_HD_U_J3.jar", forge:"Forge 54.0.34"}` |
| **OptiFine: jar** | `https://bmclapi2.bangbang93.com/optifine/{mc}/{type}/{patch}` | ✅ 206 `application/java-archive`, redirige a CDN con el filename correcto |
| Assets | `https://resources.download.minecraft.net/{hash[0..2]}/{hash}` | ✅ estándar |
| Mirror alterno | BMCLAPI espeja piston-meta / maven / assets / version json | verificar rutas exactas antes de usarlas |

### 2.1 El caso OptiFine: sí, es un parche (tenías razón)

OptiFine **no es un cargador como Fabric**. El jar que se descarga de optifine.net es a la vez
**instalador y parche**: por dentro lleva el `Patcher` que **copia el `client.jar` de Minecraft
dentro del jar de OptiFine**, y la versión resultante es ese jar parcheado. Por eso se lanza con
`net.minecraft.launchwrapper.Launch --tweakClass optifine.OptiFineTweaker`.

Cómo lo resuelven los launchers que sí lo automatizan (HMCL lo hace **sin ejecutar el instalador
con GUI**; ObsMCLauncher ejecuta el instalador como subproceso Java y parsea el stdout):

**Algoritmo de instalación standalone (el bueno, sin GUI):**

1. Listar desde BMCLAPI y descargar el jar (`OptiFine_{mc}_{edition}_{release}.jar`).
2. **Identificar el jar sin ejecutarlo**: abrir el zip y buscar `Config.class` en una de estas rutas:
   `Config.class` | `net/optifine/Config.class` | `notch/net/optifine/Config.class`.
   Parsear el *constant pool* UTF-8 del `.class` y leer los strings que siguen a las constantes
   `MC_VERSION`, `OF_EDITION`, `OF_RELEASE`. Eso da la MC objetivo y la versión de OptiFine
   (y sirve para **rechazar un jar que no corresponde a la versión elegida**).
3. **El parche**: si el jar contiene `optifine/Patcher.class` (OptiFine clásico, basado en launchwrapper):
   ```
   java -cp <installer.jar> optifine.Patcher \
        <versions/<id>/<id>.jar>   \   # client jar vanilla
        <installer.jar>            \   # fuente de parches
        <libraries/optifine/OptiFine/<mc>_<ed>_<rel>/OptiFine-<mc>_<ed>_<rel>.jar>
   ```
   Si **no** hay `Patcher.class` (OptiFine moderno): copiar el installer jar tal cual como librería.
4. Borrar `META-INF/mods.toml` del jar resultante (evita que Forge/FML lo vea como mod duplicado).
5. **launchwrapper**: si el jar trae `launchwrapper-2.0.jar` → extraerlo a `libraries/optifine/launchwrapper/2.0/`.
   Si trae `launchwrapper-of.txt` + `launchwrapper-of-<v>.jar` → extraerlo como `optifine:launchwrapper-of:<v>`.
   Si no trae ninguno → añadir la librería `net.minecraft:launchwrapper:1.12`.
6. Leer `buildof.txt` (timestamp del build) — solo necesario para el chequeo de compatibilidad
   OptiFine/Forge 1.17+ con `BootstrapLauncher`.
7. Generar el manifiesto: `mainClass = net.minecraft.launchwrapper.Launch`,
   `gameArgs += --tweakClass optifine.OptiFineTweaker`, librerías `[optifine:OptiFine:<mc>_<ed>_<rel>, launchwrapper]`.

**Modo "OptiFine como mod de Forge"** (lo que quiere la gente en 1.17+): no se parchea nada;
se copia el jar a `mods/` del gameDir. Es el camino más simple y el que conviene usar cuando el
usuario eligió Forge.

**Regla de UI que se desprende**: OptiFine + Fabric **no existe** como tal (se usa el mod
*OptiFabric*, no siempre actualizado). Si el usuario elige esa combinación, el launcher debe
advertir y ofrecer Sodium, no intentar instalarlo.

**Riesgo**: BMCLAPI es un mirror de terceros. Mitigación: fallback "arrastrá el jar que bajaste de
optifine.net" (egui soporta drag & drop nativo) + un botón que abre la página oficial.

### 2.2 Forge en detalle

1. Listar versiones desde `maven-metadata.xml`, filtrando las que empiezan por `{mc}-`.
   (No usar `promotions_slim.json`: 403 a clientes automatizados.)
2. Descargar `forge-{mc}-{forge}-installer.jar`.
3. **Extraer `install_profile.json` y `version.json` del jar sin ejecutar el instalador.**
4. Fusionar `version.json` con el vanilla → librerías, `arguments`, `mainClass`.
5. **Ejecutar los `processors`** que declara `install_profile.json`. Cada processor es
   `{ jar, classpath, args, outputs, sides }`, y hay un mapa `data` con placeholders:
   - `{MINECRAFT_JAR}`, `{ROOT}`, `{SIDE}`, `{MINECRAFT_VERSION}` → rutas/nombres nuestros.
   - `[net.minecraftforge:some:artifact]` → descargar ese artefacto del maven y sustituir por su ruta local.
   - `'literal'` → literal tal cual.
   Se ejecutan en orden con `java -cp <classpath resuelto> <mainClass> <args>`.
6. NeoForge: mismo esquema. Mapeo de versión MC→NeoForge: quitar el `1.` inicial
   (`1.21.4` → `21.4.x`, `1.20.2` → `20.2.x`). **verificar** en implementación.

**Esto es lo más pesado del proyecto.** Alternativa aceptable si los processors se atascan:
ejecutar el instalador oficial como subproceso Java en una carpeta temporal (lo que hace
ObsMCLauncher). Menos elegante, mucho más corto de implementar.

---

## 3. Estructura del proyecto

Ubicación: `cositas/mclite/` (el repo ya agrupa proyectos en `cositas/`).

```
cositas/mclite/
├── Cargo.toml
├── build.rs                      # icono + manifest Win32 (DPI, longPathAware, uac)
├── assets/icon.ico
├── README.md
├── PLAN.md                       # este archivo
└── src/
    ├── main.rs                   # #![windows_subsystem = "windows"], eframe::run_native
    ├── app.rs                    # estado global + router de pantallas
    ├── ui/
    │   ├── mod.rs
    │   ├── theme.rs              # paleta oscura, spacing, fuentes, badges
    │   ├── home.rs               # instancia seleccionada + botón JUGAR + progreso
    │   ├── new_instance.rs       # selector MC + cargador + versión + toggle snapshots
    │   ├── instances.rs          # lista lateral de instancias
    │   ├── settings.rs           # nick, Java, RAM, mirror, caché, logs
    │   └── widgets.rs            # segmented control, combo con búsqueda, progress
    ├── core/
    │   ├── mod.rs
    │   ├── paths.rs              # %APPDATA%\McLite, gameDir por instancia
    │   ├── http.rs               # cliente + reintentos + descarga paralela + sha1
    │   ├── manifest.rs           # version_manifest_v2 + filtro release/snapshot
    │   ├── version_json.rs       # structs (arguments, rules, downloads, inheritsFrom)
    │   ├── rules.rs              # evaluación de rules
    │   ├── libraries.rs          # resolución maven + natives + classpath
    │   ├── natives.rs            # extracción de natives
    │   ├── assets.rs             # asset index + objects
    │   ├── java.rs               # detección de JRE + descarga del runtime Mojang
    │   ├── auth.rs               # UUID offline + token dummy
    │   ├── instance.rs           # modelo + persistencia (instances.json)
    │   └── launch.rs             # armado del command line + spawn
    └── loaders/
        ├── mod.rs                # trait Loader
        ├── vanilla.rs
        ├── fabric.rs             # Fabric + Quilt
        ├── forge.rs              # Forge + NeoForge (install_profile + processors)
        └── optifine.rs           # BMCLAPI + Patcher + tweakClass
```

### 3.1 Crate de dependencias

| Crate | Para qué |
|---|---|
| `eframe` (feature `wgpu`) | UI. Backend **wgpu** (DirectX 12) en vez del `glow` por defecto: más robusto en drivers raros de Windows |
| `egui` | widgets, estilos |
| `tokio` + `reqwest` (feature `rustls-tls`) | HTTP async y descargas en paralelo. **rustls, no OpenSSL** |
| `serde` / `serde_json` | manifiestos |
| `zip` | abrir client jar / installer jars |
| `sha1`, `sha2` | verificación de descargas |
| `md-5` + `uuid` | UUID offline |
| `dirs` | `%APPDATA%` |
| `quick-xml` | parsear `maven-metadata.xml` de Forge |
| `embed-resource` (build-dep) | icono + manifest Win32 |
| `tracing` + `tracing-subscriber` | logs a archivo (sin consola en release) |

`Cargo.toml` en release: `lto = "fat"`, `codegen-units = 1`, `strip = true`, `panic = "abort"`, `opt-level = "s"`.

---

## 4. Modelos de datos

### 4.1 Layout en disco

```
%APPDATA%\McLite\
├── config.json                       # nick, java, RAM, mirror, tema
├── instances.json                    # índice de instancias
├── versions/<id>/<id>.json           # manifiesto final (vanilla o mergeado)
├── versions/<id>/<id>.jar            # client jar
├── libraries/…                       # estructura maven
├── assets/indexes/<id>.json
├── assets/objects/<ab>/<sha1>
├── runtime/java-runtime-delta/…      # JRE de Mojang
└── instances/<slug>/                 # gameDir real
    ├── instance.json                 # { name, mc, loader, loaderVersion, ram, width, height, javaPath }
    ├── mods/ saves/ resourcepacks/ options.txt …
```

### 4.2 Trait de cargador

```rust
pub enum LoaderKind { Vanilla, Fabric, Quilt, Forge, NeoForge, OptiFine }

pub struct LoaderVersion {
    pub id: String,        // "0.19.5" | "54.1.6" | "HD_U_J3"
    pub stable: bool,
    pub mc: String,
}

#[async_trait]
pub trait Loader {
    fn kind(&self) -> LoaderKind;
    /// Versiones del cargador compatibles con `mc`, más nuevas primero.
    async fn list_versions(&self, mc: &str) -> Result<Vec<LoaderVersion>>;
    /// Genera/instala el manifiesto de la instancia (puede implicar descargas y processors).
    async fn install(&self, ctx: &InstallCtx, mc: &str, ver: &LoaderVersion, progress: &Progress) -> Result<VersionJson>;
}
```

`install` devuelve un `VersionJson` ya resuelto (post-herencia) que `launch.rs` consume igual
para los seis cargadores. Esa uniformidad es lo que mantiene el resto del código simple.

### 4.3 Herencia (`inheritsFrom`)

Fabric/Quilt/Forge modernos devuelven un json con `inheritsFrom: "<mc>"`. Hay que:
cargar el padre (del manifiesto vanilla), **concatenar** `libraries` (las del hijo primero),
unir `arguments` (`game`/`jvm`), y tomar del padre `assets`, `assetIndex`, `downloads.client`,
`javaVersion` salvo que el hijo los sobreescriba. Esta función es la base de Fabric *y* de Forge,
por eso vale la pena hacerla bien en la Fase 2 y no improvisarla.

### 4.4 Reglas y argumentos

```rust
// rules vacías => permitido. Se evalúan en orden y la última que matchea gana.
fn allowed(rules: &[Rule], f: &Features) -> bool {
    let mut allow = false;
    for r in rules {
        if r.matches(f) { allow = r.action == RuleAction::Allow; }
    }
    allow
}
```
`r.matches` filtra por `os.name` (`windows`), `os.arch` (`x86_64`), `os.version` (regex) y
`features` (`is_demo_user`, `has_custom_resolution`, `has_quick_plays_support`… → todos `false`).

Placeholders a sustituir en `arguments.jvm` y `arguments.game`:
`${natives_directory}`, `${classpath}`, `${launcher_name}`, `${launcher_version}`,
`${auth_player_name}`, `${version_name}`, `${game_directory}`, `${assets_root}`,
`${assets_index_name}`, `${auth_uuid}`, `${auth_access_token}`, `${user_type}`, `${version_type}`,
`${resolution_width}`, `${resolution_height}`, `${clientid}`, `${xuid}`, `${auth_session}`.

Notas finas que rompen el launch si se ignoran:
- `${resolution_width}`/`${height}` **siempre** hay que pasarlos (default 854x480 si el usuario no eligió) — si van vacíos, quedan `--width` sin valor y el juego no arranca.
- Versiones **< 1.13** usan `minecraftArguments` (string) en vez de `arguments` → hay que soportar ambos y añadir `--userProperties {}`.
- `logging.client.file` (1.12–1.16): descargar el log4j config y pasar `-Dlog4j.configurationFile=<ruta>` con el `id` sustituido.
- Seguridad: para versiones 1.7–1.16 añadir `-Dlog4j2.formatMsgNoLookups=true` (Log4Shell).
- Classpath: separador `;` en Windows, y el client jar va **al final**.

### 4.5 Cuenta offline

```rust
// Java: UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(UTF_8))  => MD5 con bits v3
fn offline_uuid(name: &str) -> Uuid {
    let mut md5 = Md5::new();
    md5.update(format!("OfflinePlayer:{name}"));
    let mut b: [u8; 16] = md5.finalize().into();
    b[6] = (b[6] & 0x0f) | 0x30;  // versión 3
    b[8] = (b[8] & 0x3f) | 0x80;  // variante RFC 4122
    Uuid::from_bytes(b)
}
```
`--accessToken 0`, `--userType msa` (y `legacy` para < 1.7). Con esto se juega singleplayer y
servers con `online-mode=false`; los servers con `online-mode=true` rechazan, y eso es esperado
según la decisión tomada.

---

## 5. UI / UX

Ventana 980×620 (min 820×520), resizable, tema oscuro con acento verde Minecraft (`#3C8527`) y
badges de color por cargador. Panel lateral izquierdo = lista de instancias; centro = contenido.

**Pantalla 1 — Inicio.** Tarjeta grande de la instancia seleccionada (nombre, MC, badge de cargador,
RAM asignada), botón **JUGAR** grande, barra de progreso con fase textual
(`Descargando libraries 45/212`), y un log colapsable para ver stdout de Java.

**Pantalla 2 — Nueva instancia.**
- Nombre autogenerado (`Fabric 1.21.4`), editable.
- **Versión de Minecraft**: combo con búsqueda y badge `latest release` / `latest snapshot`.
  - Checkbox **"Mostrar snapshots"** — por defecto **OFF**:
    - OFF → solo `type == "release"`.
    - ON → se agrega una sección *Snapshots*, y otra colapsada *Beta/Alpha* (off por defecto).
- **Cargador**: segmented control `Vanilla | Fabric | Quilt | Forge | NeoForge | OptiFine`.
- **Versión del cargador**: combo async (con spinner mientras carga), con `Latest` como default.
  Los combos incompatibles se **deshabilitan**, no se ocultan: NeoForge solo ≥ 1.20.2,
  OptiFine+Fabric muestra advertencia (OptiFabric / usar Sodium).
- Opciones: RAM 1–16 GB, resolución, carpeta, "abrir carpeta al terminar".
- Botón **Crear** → crea `gameDir` + `instance.json` y descarga en background.

**Pantalla 3 — Ajustes.** Nick, Java (auto / detección / ruta manual), RAM por defecto,
mirror on/off, borrar caché, abrir logs, borrar instancia.

Extras baratos que suman mucho: drag & drop del jar de OptiFine sobre la ventana, y un
"Reinstalar/Reparar" por instancia (borra el manifiesto y vuelve a resolver).

---

## 6. Build y distribución en Windows

- `#![windows_subsystem = "windows"]` en `main.rs` para no mostrar consola en release. En debug sí
  se quiere consola → condicionarlo con `#[cfg(debug_assertions)]`.
- `build.rs` con `embed-resource`: icono, `VERSIONINFO`, y manifest con **per-monitor-v2 DPI**
  (`longPathAware: true` — con mods y mundos anidados se pasa de 260 chars fácilmente, y el
  launcher oficial de Mojang también lo activa).
- Spawn de Java: `std::os::windows::process::CommandExt::creation_flags(0x00000200)` (nuevo grupo
  de procesos) y leer stdout/stderr hacia el log de la UI.
- **Compilar desde Linux** (el entorno actual) con `cargo-xwin`:
  ```
  cargo install cargo-xwin
  cargo xwin build --release --target x86_64-pc-windows-msvc
  ```
  Alternativa: target `x86_64-pc-windows-gnu` con `mingw-w64`. Con `rustls` no hay dolor de OpenSSL.
- CI: agregar `cositas/.github/workflows/ci-launcher.yml` con runner `windows-latest`
  (`cargo build --release` + `cargo test` + artifact). Ojo: el repo tiene los workflows bajo
  `cositas/.github/` y **GitHub solo lee `.github/workflows` en la raíz** — si se quiere CI real hay
  que crear también `.github/workflows/` en la raíz.
- Sin firmar, Windows SmartScreen va a avisar ("editor desconocido"). Documentarlo en el README, o
  firmar más adelante.

---

## 7. Roadmap por fases

| Fase | Entregable | Riesgo |
|---|---|---|
| **0. Andamiaje** | `cargo` project, deps, `build.rs` (icono+manifest), ventana eframe con tema oscuro, `.gitignore`, CI | bajo |
| **1. Vanilla offline end-to-end** ← *el hito que valida todo* | `manifest.rs`, `version_json.rs`, `rules.rs`, `libraries.rs`, `assets.rs`, `natives.rs`, `java.rs` (solo detección), `auth.rs`, `launch.rs`. UI: inicio + selector de versión con **toggle de snapshots** + JUGAR + progreso. **Meta: lanzar 1.21.4 offline** | medio |
| **2. Fabric + Quilt** | `loaders/fabric.rs` con `profile/json` y **resolución de herencia** (que Forge reutiliza después). Quilt sale casi gratis del mismo código | bajo |
| **3. Runtime de Java propio** | Descarga de `java-runtime-*` desde Mojang (`all.json` → manifest por plataforma → extraer respetando el flag `executable`), selección por `javaVersion.majorVersion` del version json. Ya no depende de tener Java instalado | bajo-medio |
| **4. Forge + NeoForge** | `maven-metadata.xml`, installer jar, `install_profile.json`, resolución de placeholders, ejecución de `processors` en orden, merge | **alto** |
| **5. OptiFine** | BMCLAPI list+download, identificación por constant pool, `optifine.Patcher`, launchwrapper, `--tweakClass`, modo mod-de-Forge, fallback drag & drop del jar | medio |
| **6. Pulido** | UI final (badges, animaciones suaves, estados vacíos), log viewer, mensajes de error humanos, reparar/borrar instancia, optimización del binario, release en CI | bajo |

**Por qué este orden:** la Fase 1 construye el 70 % de la lógica (descarga, herencia, argumentos,
launch) y da algo jugable. Fabric (Fase 2) es el cargador más fácil y ya ejercita el merge.
Forge (Fase 4) es el único con riesgo real de cronograma: si los processors se complican, el
fallback es ejecutar el instalador oficial como subproceso (patrón ObsMCLauncher) y seguir.

---

## 8. Riesgos y mitigaciones

| Riesgo | Mitigación |
|---|---|
| **Processors de Forge** (placeholders `[maven:…]`, orden, outputs) mal resueltos | Aislarlos detrás de `trait Loader`; fallback a ejecutar el instalador oficial en carpeta temporal |
| **Forge `promotions_slim.json` 403** | Usar `maven-metadata.xml` (verificado 200), no `promotions_*` |
| **OptiFine depende de un mirror de terceros** (BMCLAPI) | Fallback obligatorio: drag & drop del jar oficial + botón a optifine.net. El flujo de instalación funciona igual con un jar local |
| **Assets ~1 GB en 1.21+** en la primera instalación | Descarga paralela (8–16 conexiones), `assets/` y `libraries/` compartidos entre instancias, reanudable y con verificación SHA-1 |
| Versiones muy viejas (alpha/beta) rompen el flujo moderno | Soportar `minecraftArguments`, pero declarar **experimental** todo lo anterior a 1.7 en la v1 |
| Reglas/argumentos cambian con cada release | Tests de snapshot contra 3 versiones fijas (1.8.9, 1.16.5, 1.21.4) en CI |
| Rutas de BMCLAPI cambian | Marcadas **verificar** en la tabla §2; toda URL externa vive en un módulo `endpoints.rs` para cambiarla en un solo lugar |
| Legalidad | No se redistribuye nada de Mojang: el launcher descarga de los servidores oficiales. El modo offline no rompe el EULA |

---

## 9. Criterios de aceptación (checklist de "hecho")

- [ ] Abre y cierra en < 1 s; `.exe` < 15 MB; RAM de UI < 150 MB.
- [ ] Con el checkbox de snapshots **apagado**, la lista no muestra ningún `type != release`; **encendido**, aparecen los snapshots.
- [ ] Lanza **vanilla 1.21.4** offline en un Windows sin Java instalado.
- [ ] Lanza **Fabric 1.21.4** con un mod real (`mods/sodium.jar`) y el mod carga.
- [ ] Lanza **Forge 1.20.1** con un mod de prueba.
- [ ] Lanza **OptiFine 1.21.4** standalone y aparece *Video Settings* de OptiFine en el menú.
- [ ] Lanza **OptiFine en modo mod de Forge** copiándolo a `mods/`.
- [ ] Verifica SHA-1 de todo lo descargado y reintenta con backoff si una descarga falla.
- [ ] Mensaje de error legible si no hay red / si el cargador elegido no soporta esa versión de MC.

---

## 10. Fuera de alcance de la v1

Login Microsoft, skins/capas online, gestor de mods desde CurseForge/Modrinth, modpacks,
instalación de Java por versión múltiple en paralelo, auto-update del launcher, Linux/macOS,
y versiones < 1.7 declaradas estables.
