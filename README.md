# McLite

Launcher ligero de Minecraft para Windows. Un solo `.exe` portable, sin instalador, sin .NET y sin Java preinstalado: lo baja solo cuando hace falta.

![Estado](https://img.shields.io/badge/estado-beta-orange) ![Plataforma](https://img.shields.io/badge/plataforma-Windows%20x64-blue) ![Licencia](https://img.shields.io/badge/licencia-propietaria-red)

## Características

- **Un ejecutable portable**: descárgalo, ejecútalo. La configuración vive en una carpeta `mclite/` junto al exe (si la carpeta no es escribible, cae a `%APPDATA%\mclite`).
- **6 cargadores**: Vanilla, Fabric, Quilt, Forge, NeoForge y OptiFine.
- **Modpacks de Modrinth**: búsqueda con iconos, detalle e instalación de `.mrpack` (mods + overrides) en una instancia nueva.
- **Java automático**: usa el Java del sistema si sirve; si no, baja el runtime oficial de Mojang (`java-runtime-*`) igual que el launcher oficial. No hay que instalar nada.
- **Sodium a un clic** para instancias Fabric (y casilla al crear la instancia).
- **Cuentas offline** con nick local (UUID offline estándar). Ideal para jugar en singleplayer y servidores con `online-mode=false`.
- **Descargas verificadas**: SHA-1 en todo, descarga en paralelo con reintentos y progreso con velocidad y ETA.
- **Diagnóstico**: log del launcher rotado, espejo del log del juego, crash reports guardados y clasificados por causa.
- **Instancias editables**: nombre, RAM, resolución, versión de MC y cargador — guardar reinstala lo que falte.
- UI oscura con tema verde Minecraft, esquinas suaves y badges por cargador.

## Descarga

Baja el `mclite.exe` más reciente de la página de [Releases](../../releases).

- **Tamaño**: ~8,3 MB. No requiere instalación.
- Windows SmartScreen puede avisar ("editor desconocido") porque el binario no está firmado: pulsa *Más información → Ejecutar de todas formas*.

## Uso rápido

1. Ejecuta `mclite.exe`.
2. En **Ajustes**, escribe tu nick (cuenta offline, 3–16 caracteres).
3. **Nueva instancia** → elige versión de MC, cargador y RAM → **Crear**.
4. **Jugar**. La primera vez descarga el juego (~500 MB–1 GB); las siguientes arrancan directo.

Si mueves o reemplazas el `mclite.exe`, el launcher **importa automáticamente** tu configuración e instancias desde la carpeta de datos anterior (carpeta `mclite/` cercana o `%APPDATA%\mclite`).

## Requisitos

- Windows 10/11 x64. Render por DirectX 12 o Vulkan (wgpu), con fallback GLES.
- Java: **no hace falta**. Se detecta el del sistema o se baja el runtime de Mojang para la versión que pida cada juego (jre-legacy para ≤ 1.8, beta para 1.9–1.17, delta/epsilon para 1.18+).

## Compilar

```bash
# Linux (cross-compile a Windows) — ver scripts/build-windows.sh
./scripts/build-windows.sh

# Nativo (Windows con MSVC o mingw)
cargo build --release
```

El binario queda en `target/x86_64-pc-windows-gnu/release/mclite.exe` (cross) o `target/release/mclite.exe` (nativo).

Tests del núcleo (sin GUI):

```bash
cargo test --no-default-features
```

## Estructura

```
src/
├── main.rs            # CLI (open, crashes, root…) + arranque de la GUI
├── app.rs             # estado de la ventana, mensajes de hilos de fondo
├── core/              # sin UI: paths, http, manifiestos, assets, runtime, modrinth…
└── loaders/           # vanilla, fabric+quilt, forge+neoforge, optifine
```

Cada cargador genera un manifiesto de versión ya resuelto (merge de herencia incluido) que el lanzador consume igual para los seis. No se redistribuye ningún archivo de Mojang: todo se descarga de sus servidores oficiales (y BMCLAPI solo como espejo para OptiFine).

## Licencia

Propietaria: se permite usar el programa tal cual, pero **no redistribuirlo** ni reutilizar su código. Ver [LICENSE](LICENSE).
