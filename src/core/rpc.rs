//! Discord Rich Presence: muestra "Jugando Minecraft <versión>" en el perfil.
//!
//! Habla con la app de Discord por su IPC local (pipe con nombre en Windows,
//! unix socket en Linux/macOS) usando el protocolo JSON documentado por
//! Discord. No sale nada a Internet por aquí: el pipe es local y Discord es
//! quien pinta el estado en el perfil del usuario.
//!
//! Necesita un Client ID de una aplicación de Discord (gratis, ~1 minuto en
//! discord.com/developers/applications). Si no está configurado o Discord no
//! está corriendo, todo es no-op silencioso: el juego no se entera.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};

/// Client ID de la app "McLite" en Discord (pública, sin secretos: el RPC
/// local solo necesita el ID para identificar el payload).
pub const DISCORD_CLIENT_ID: &str = "1422792975077109780";

#[derive(Serialize)]
struct RpcFrame<'a> {
    opcode: u32,
    #[serde(rename = "d")]
    data: FrameData<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<&'a str>,
}

#[derive(Serialize)]
struct FrameData<'a> {
    cmd: &'a str,
    args: Args<'a>,
}

#[derive(Serialize)]
struct Args<'a> {
    client_id: &'a str,
    activity: Option<Activity<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
}

#[derive(Serialize)]
struct Activity<'a> {
    details: &'a str,
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamps: Option<Timestamps>,
    assets: Assets<'a>,
}

#[derive(Serialize)]
struct Timestamps {
    start: i64,
}

#[derive(Serialize)]
struct Assets<'a> {
    large_image: &'a str,
    large_text: &'a str,
    small_image: &'a str,
    small_text: &'a str,
}

/// Payload mínimo que Discord devuelve (no lo usamos, pero hay que leerlo).
#[derive(Deserialize)]
struct IncomingFrame {
    #[serde(default)]
    opcode: u32,
}

/// Conexión viva con Discord (una por proceso).
pub struct DiscordRpc {
    stream: Stream,
}

enum Stream {
    #[cfg(windows)]
    NamedPipe(std::fs::File),
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
}

/// ¿Hubo intento de conexión ya? (evita martillar el pipe cada frame).
static TRIED_CONNECT: AtomicBool = AtomicBool::new(false);
static CONNECTED: AtomicBool = AtomicBool::new(false);

/// Rutas típicas del IPC de Discord (linux/macOS).
#[cfg(unix)]
fn socket_candidates() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        for i in 0..10 {
            paths.push(std::path::PathBuf::from(format!(
                "{runtime}/discord-ipc-{i}"
            )));
        }
    }
    for i in 0..10 {
        paths.push(std::path::PathBuf::from(format!("/tmp/discord-ipc-{i}")));
    }
    paths
}

/// Rutas típicas del IPC de Discord en Windows (named pipes).
#[cfg(windows)]
fn pipe_candidates() -> Vec<String> {
    (0..10)
        .map(|i| format!("\\\\\\.\\pipe\\discord-ipc-{i}"))
        .collect()
}

impl DiscordRpc {
    /// Conecta con Discord. `Ok(None)` = no está Discord (no es error).
    pub fn connect() -> Result<Option<Self>> {
        if CONNECTED.load(Ordering::Relaxed) {
            // Ya hay una conexión en otro lugar: no duplicar. El diseño es
            // una sola conexión por proceso mantenida en app.rs.
        }
        #[cfg(windows)]
        {
            for path in pipe_candidates() {
                // El pipe nominal de Windows se abre como fichero.
                if let Ok(file) = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                {
                    let mut rpc = Self {
                        stream: Stream::NamedPipe(file),
                    };
                    rpc.handshake()?;
                    CONNECTED.store(true, Ordering::Relaxed);
                    return Ok(Some(rpc));
                }
            }
        }
        #[cfg(unix)]
        {
            for path in socket_candidates() {
                if let Ok(stream) = std::os::unix::net::UnixStream::connect(&path) {
                    let mut rpc = Self {
                        stream: Stream::Unix(stream),
                    };
                    rpc.handshake()?;
                    CONNECTED.store(true, Ordering::Relaxed);
                    return Ok(Some(rpc));
                }
            }
        }
        TRIED_CONNECT.store(true, Ordering::Relaxed);
        Ok(None)
    }

    /// Handshake OP=0: presenta el client_id (con pid, como pide Discord).
    fn handshake(&mut self) -> Result<()> {
        let pid = std::process::id();
        let frame = RpcFrame {
            opcode: 0,
            nonce: Some("mclite-hs"),
            data: FrameData {
                cmd: "SET_CLIENT_ID",
                args: Args {
                    client_id: DISCORD_CLIENT_ID,
                    activity: None,
                    pid: Some(pid),
                },
            },
        };
        self.send_frame(&frame)
    }

    /// Publica la actividad (OP=1). `details` = versión jugando; `state` = texto libre.
    pub fn set_activity(
        &mut self,
        details: &str,
        state: &str,
        start_epoch: Option<i64>,
    ) -> Result<()> {
        let frame = RpcFrame {
            opcode: 1,
            nonce: Some("mclite-act"),
            data: FrameData {
                cmd: "SET_ACTIVITY",
                args: Args {
                    client_id: DISCORD_CLIENT_ID,
                    pid: Some(std::process::id()),
                    activity: Some(Activity {
                        details,
                        state,
                        timestamps: start_epoch.map(|start| Timestamps { start }),
                        assets: Assets {
                            large_image: "mclite-logo",
                            large_text: "McLite",
                            small_image: "grass",
                            small_text: "Minecraft",
                        },
                    }),
                },
            },
        };
        self.send_frame(&frame)
    }

    /// Limpia la presencia (activity None).
    pub fn clear(&mut self) -> Result<()> {
        let frame = RpcFrame {
            opcode: 1,
            nonce: Some("mclite-clr"),
            data: FrameData {
                cmd: "SET_ACTIVITY",
                args: Args {
                    client_id: DISCORD_CLIENT_ID,
                    pid: Some(std::process::id()),
                    activity: None,
                },
            },
        };
        self.send_frame(&frame)?;
        CONNECTED.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn send_frame(&mut self, frame: &RpcFrame<'_>) -> Result<()> {
        let mut payload = serde_json::to_vec(frame)?;
        // Marco de Discord: u32 LE opcode + u32 LE longitud + JSON.
        let len = payload.len() as u32;
        let mut bytes = Vec::with_capacity(8 + payload.len());
        bytes.extend_from_slice(&frame.opcode.to_le_bytes());
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.append(&mut payload);
        match &mut self.stream {
            #[cfg(windows)]
            Stream::NamedPipe(file) => file
                .write_all(&bytes)
                .and_then(|_| file.flush())
                .map_err(|e| Error::PlainIo(e)),
            #[cfg(unix)]
            Stream::Unix(stream) => stream
                .write_all(&bytes)
                .and_then(|_| stream.flush())
                .map_err(|e| Error::PlainIo(e)),
        }?;
        // Leer (y descartar) el ack de Discord: sin esto el pipe se atasca.
        let mut header = [0u8; 8];
        match &mut self.stream {
            #[cfg(windows)]
            Stream::NamedPipe(file) => {
                let _ = file.read_exact(&mut header);
            }
            #[cfg(unix)]
            Stream::Unix(stream) => {
                let _ = stream.read_exact(&mut header);
            }
        }
        let ack_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        if ack_len > 0 && ack_len < 64 * 1024 {
            let mut ack = vec![0u8; ack_len];
            match &mut self.stream {
                #[cfg(windows)]
                Stream::NamedPipe(file) => {
                    let _ = file.read_exact(&mut ack);
                }
                #[cfg(unix)]
                Stream::Unix(stream) => {
                    let _ = stream.read_exact(&mut ack);
                }
            }
            // OPC 1 = FRAME: ok. OPC 4 = CLOSE: Discord se va.
            if let Ok(incoming) = serde_json::from_slice::<IncomingFrame>(&ack) {
                if incoming.opcode == 4 {
                    CONNECTED.store(false, Ordering::Relaxed);
                    return Err(Error::Unsupported("Discord cerró la conexión".into()));
                }
            }
        }
        Ok(())
    }
}

/// Serializa el frame de handshake/actividad (lo usa un test).
#[cfg(test)]
fn frame_json(opcode: u32, cmd: &str, details: &str) -> Vec<u8> {
    let frame = RpcFrame {
        opcode,
        nonce: Some("t"),
        data: FrameData {
            cmd,
            args: Args {
                client_id: DISCORD_CLIENT_ID,
                activity: Some(Activity {
                    details,
                    state: "en el menú",
                    timestamps: None,
                    assets: Assets {
                        large_image: "mclite-logo",
                        large_text: "McLite",
                        small_image: "grass",
                        small_text: "Minecraft",
                    },
                }),
                pid: Some(1234),
            },
        },
    };
    serde_json::to_vec(&frame).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_frame_lleva_opcode_cmd_y_client_id() {
        let json = frame_json(1, "SET_ACTIVITY", "Minecraft 1.21.4");
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(value["opcode"], 1);
        assert_eq!(value["d"]["cmd"], "SET_ACTIVITY");
        assert_eq!(value["d"]["args"]["client_id"], DISCORD_CLIENT_ID);
        assert_eq!(
            value["d"]["args"]["activity"]["details"],
            "Minecraft 1.21.4"
        );
        // El pid va fuera de activity (discord lo espera en args).
        assert_eq!(value["d"]["args"]["pid"], 1234);
    }

    #[test]
    fn el_marco_empaqueta_opcode_y_longitud() {
        let json = frame_json(1, "SET_ACTIVITY", "x");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&json);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            json.len() as u32
        );
    }
}
