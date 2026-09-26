# 🟩 McLite

**Launcher ligero de Minecraft para Windows.** Un solo `.exe` portable, sin instalador, sin .NET y sin Java preinstalado: lo baja solo cuando hace falta.

![Versión](https://img.shields.io/badge/versi%C3%B3n-0.8.0-green) ![Estado](https://img.shields.io/badge/estado-beta-orange) ![Plataforma](https://img.shields.io/badge/plataforma-Windows%20x64-blue) ![Licencia](https://img.shields.io/badge/licencia-propietaria-red)

---

## ✨ Qué ofrece

| | |
|---|---|
| 🪟 **Portable de verdad** | Un ejecutable. La configuración vive en una carpeta `mclite/` junto al exe (o `%APPDATA%\mclite` si la carpeta no es escribible). |
| 🧩 **6 cargadores** | Vanilla, Fabric, Quilt, Forge, NeoForge y OptiFine. |
| 📦 **Modpacks de Modrinth** | Búsqueda con iconos, detalle e instalación de `.mrpack` en una instancia nueva. También arrastra un `.mrpack` a la ventana. |
| ☕ **Java automático** | Usa el del sistema si sirve; si no, baja el runtime oficial de Mojang. Nada que instalar. |
| ⚡ **Sodium a un clic** | Para instancias Fabric (y casilla al crear). |
| 👤 **Dos tipos de cuenta** | Offline con nick local, o **cuenta Microsoft real** por device code (nombre, UUID y skin de Mojang). |
| 🛠️ **Gestor de mods** | Activa/desactiva/borra los `.jar` de una instancia sin abrir carpetas. |
| 📸 **Galería de capturas** | Miniaturas por fecha, visor en grande, borrar, abrir carpeta. |
| 💾 **Copias de seguridad** | Exporta la instancia a un `.zip` (mundos, mods, config) y restaúrala arrastrándolo a la ventana. |
| 🎮 **Discord Rich Presence** | "Minecraft 1.21.4 — con McLite" en tu perfil mientras juegas (apagable). |
| 🔄 **Auto-update** | Aviso en el arranque, descarga verificada por SHA-256, cierre solo, instalación y re-apertura con rollback configurable. |
| 🩺 **Diagnóstico** | Log del launcher, espejo del log del juego, crash reports clasificados por causa. |

**Descargas verificadas** (SHA-1 en el juego, SHA-256 en el updater), descarga en paralelo con reintentos, y UI oscura con tema verde Minecraft y secciones en acordeón.

## 📥 Descarga

Baja el `mclite.exe` más reciente de la página de [**Releases**](../../releases).

- **Tamaño**: ~10 MB. No requiere instalación.
- Windows SmartScreen puede avisar ("editor desconocido") porque el binario no está firmado: pulsa *Más información → Ejecutar de todas formas*.
- Cada release trae `mclite.exe.sha256`: si quieres verificarlo, `certutil -hashfile mclite.exe SHA256` debe coincidir.

## 🚀 Uso rápido

1. Ejecuta `mclite.exe`.
2. La primera vez, el onboarding te pide tu nick y tu color favorito.
3. **Nueva instancia** → versión de MC, cargador y RAM → **Crear**.
4. **Jugar**. La primera partida descarga el juego (~500 MB–1 GB); las siguientes arrancan directo.

> Si mueves o reemplazas el `mclite.exe`, el launcher **importa automáticamente** tu configuración e instancias desde la carpeta de datos anterior.

### Cuenta Microsoft (opcional)

Para jugar con tu nombre, UUID y skin reales necesitas un **Client ID gratuito de Azure** (5 minutos, solo la primera vez). Guía paso a paso: **[docs/MICROSOFT-ACCOUNT.md](docs/MICROSOFT-ACCOUNT.md)**. Sin él, el launcher funciona igual con la cuenta offline.

## 🧭 Trucos

- **Doble clic** en una instancia del sidebar = jugar.
- **Clic derecho** = Jugar/Editar/Reparar/Carpeta/Borrar.
- **Arrastra a la ventana**: `.mrpack` → instala el pack · `.jar` → añade el mod (instancias con cargador) · `.png` → aplica la skin · `.zip` de backup → restaura la instancia.
- **Atajos**: `Ctrl+Enter` jugar · `Ctrl+N` nueva instancia · `Esc` volver.
- El exe viejo tras una actualización queda como `mclite.exe.old` (rollback); se limpia al arrancar según la ventana que elijas en **Ajustes → Actualizaciones**.

## 📋 Requisitos

- Windows 10/11 x64. Render por DirectX 12 o Vulkan (wgpu), con fallback GLES.
- Java: **no hace falta** (ver arriba).

## 🗂️ Estructura de datos

```
mclite/
├── mclite.exe            ← el launcher
├── mclite/               ← datos (config, instancias, caché)
│   ├── config.json
│   ├── instances.json
│   ├── instances/<slug>/ ← cada instancia (game dir)
│   ├── cache/icons/      ← iconos de Modrinth
│   ├── cache/skins/      ← skins offline
│   └── backups/          ← copias .zip exportadas
└── logs/launcher.log
```

## 🧱 Cómo está hecho

Rust + egui/eframe (wgpu), sin `.NET` ni frameworks pesados. Módulos principales:

- `core/` — instalación, lanzamiento, cargadores, manifiesto, skins, updater, MSA, mods, backups, RPC. Sin dependencias de UI: la CLI (`mclite help`) usa lo mismo que la ventana.
- `ui/` — pantallas egui con el tema propio (acordeón, tarjetas, barra de estado).

```bash
cargo test --no-default-features   # suite de tests
./scripts/build-windows.sh         # build de release + .sha256
```

## 📄 Licencia

Propietaria — ver [LICENSE](LICENSE). Minecraft es de Mojang/Microsoft; este launcher no está afiliado a ellos.
