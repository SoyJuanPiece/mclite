# Guía: jugar con tu cuenta de Microsoft en McLite

McLite soporta cuentas Microsoft con el **device code flow**: pegas un código
corto en el navegador y el launcher obtiene tu nombre, UUID y skin reales.

Esta configuración se hace **una sola vez** y toma ~5 minutos.

## Paso 1 — Crear el Client ID en Azure (gratis)

1. Entra a <https://portal.azure.com> con tu cuenta Microsoft (la del Minecraft).
2. Busca **Registros de aplicaciones** (App registrations) y pulsa **+ Nuevo registro**.
3. Rellena:
   - **Nombre**: `McLite` (o el que quieras).
   - **Tipos de cuenta admitidos**: **Cuentas personales de Microsoft solamente**.
   - **URI de redirección**: déjalo vacío (no hace falta para device code).
4. Pulsa **Registrar**. En la página del registro verás
   **Id. de aplicación (cliente)** — ese es tu **Client ID**. Cópialo.

## Paso 2 — Dar los permisos

1. En tu registro: **Permisos de API** → **+ Agregar un permiso**.
2. Pestaña **API que usa mi organización** → busca **Xbox Live** →
   selecciona **XboxLive.signin** → Agregar permisos.
3. Vuelve a **Permisos de API** → + Agregar un permiso → **API de Microsoft** →
   **Microsoft Graph** → **Permisos delegados** → marca `offline_access` → Agregar permisos.

   > Si no encuentras Xbox Live en la lista, también funciona añadir la URL
   > `https://login.live.com` → Permisos → XboxLive.signin desde
   > **Autenticación**. El device code flow pide el alcance
   > `XboxLive.signin offline_access` directamente.

## Paso 3 — Activar el device code flow

1. En tu registro: **Autenticación** → **Configuración avanzada** →
   **Permitir flujos de cliente públicos** → **Sí** → Guardar.
2. (Recomendado) **Autenticación** → * Allow public client flows * arriba,
   y en **URI de redirección** añade `https://login.microsoftonline.com/common/oauth2/nativeclient`.

## Paso 4 — Pegar el Client ID en McLite

1. Abre McLite → **Ajustes → Cuenta Microsoft**.
2. Pega tu **Client ID** en el campo y se guarda solo.
3. Pulsa **Iniciar sesión con Microsoft**:
   - Se abre `microsoft.com/link` (o ábrelo tú y copia el código que muestra McLite).
   - Escribe el código, inicia sesión con tu cuenta Minecraft y acepta.
4. ¡Listo! Ajustes mostrará tu nombre real y cada partida saldrá con tu skin,
   tu nombre y tu UUID de Mojang.

## Notas

- El **refresh token** se guarda en `msa-session.json` dentro de la carpeta de
  datos del launcher, en tu PC. Nunca se envía a ningún servidor ajeno a
  Microsoft/Mojang. Puedes cerrar sesión desde el mismo panel.
- El token de acceso a Minecraft caduca ~24 h; McLite lo renueva automáticamente
  al lanzar el juego.
- Si el login falla con un mensaje de "menor de edad" o "verificación", es una
  restricción de Xbox Live de tu cuenta (no de McLite): entra en
  <https://www.xbox.com> con la cuenta, completa lo que pida y reintenta.
- ¿Problemas? Abre un issue en <https://github.com/SoyJuanPiece/mclite/issues>.
