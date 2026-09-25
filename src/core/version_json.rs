//! Manifiesto de una versión (`versions/<id>/<id>.json`) y su fusión por herencia.
//!
//! Los perfiles de cargador (Fabric, Forge) traen `inheritsFrom`: hay que resolver el
//! padre y combinar. Las reglas de la fusión, que es donde se rompen los launchers:
//!
//! * escalares → gana el hijo, y si el hijo no los trae, se heredan del padre;
//! * `libraries` → **hijo primero** (sus clases deben ganar en el classpath);
//! * `arguments` → **padre primero, hijo después** (el hijo sobreescribe al final).

use serde::{Deserialize, Serialize};

use crate::core::libraries::Library;
use crate::core::rules::{self, Environment, Features, Rule};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub main_class: String,

    /// Nombre de la carpeta de assets (`assets/indexes/<assets>.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexInfo>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<Library>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,

    /// Formato anterior a 1.13: una sola cadena con todos los argumentos del juego.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersion>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,

    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub version_type: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compliance_level: Option<i32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_time: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_launcher_version: Option<i32>,

    /// JSON original del que salió esta versión. Sirve para depurar y para reescribir
    /// el perfil del cargador tal cual lo publica su autor. No se serializa.
    #[serde(skip)]
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientDownload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<ClientDownload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDownload {
    /// Solo lo usa la entrada de log4j (`client-1.21.2.xml`), que es con lo que se
    /// nombra el fichero en disco.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    #[serde(default)]
    pub component: String,
    #[serde(default)]
    pub major_version: u32,
}

/// Config de log4j. El instalador de Forge genera `"logging": {}` sin `client`,
/// así que el subcampo es opcional: sin él no hay config que bajar y punto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClient {
    /// Plantilla con `${path}`, p. ej. `-Dlog4j.configurationFile=${path}`.
    #[serde(default)]
    pub argument: String,
    pub file: ClientDownload,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgValue>,
    #[serde(default)]
    pub jvm: Vec<ArgValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    /// Argumento fijo.
    Plain(String),
    /// Argumento con condiciones (`rules`).
    Conditional(ArgConditional),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgConditional {
    #[serde(default)]
    pub rules: Vec<Rule>,
    pub value: ArgList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgList {
    One(String),
    Many(Vec<String>),
}

impl ArgList {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            ArgList::One(s) => vec![s.clone()],
            ArgList::Many(v) => v.clone(),
        }
    }
}

impl ArgValue {
    /// Devuelve los argumentos que aplican, o vacío si las reglas no se cumplen.
    pub fn expand(&self, env: &Environment, features: &Features) -> Vec<String> {
        match self {
            ArgValue::Plain(value) => vec![value.clone()],
            ArgValue::Conditional(cond) => {
                if rules::allowed(Some(&cond.rules), env, features) {
                    cond.value.to_vec()
                } else {
                    Vec::new()
                }
            }
        }
    }
}

impl VersionJson {
    pub fn from_slice(bytes: &[u8]) -> crate::Result<Self> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let mut parsed: VersionJson = serde_json::from_value(value.clone())?;
        parsed.raw = Some(value);
        Ok(parsed)
    }

    pub fn from_str(raw: &str) -> crate::Result<Self> {
        Self::from_slice(raw.as_bytes())
    }

    /// ¿Depende de otra versión?
    pub fn parent_id(&self) -> Option<&str> {
        self.inherits_from.as_deref()
    }

    /// Combina `parent` con `child` (el hijo gana) y devuelve una versión ya autocontenida.
    pub fn merge(parent: &VersionJson, child: &VersionJson) -> VersionJson {
        let mut out = child.clone();
        out.inherits_from = None;

        macro_rules! inherit {
            ($field:ident) => {
                if out.$field.is_none() {
                    out.$field = parent.$field.clone();
                }
            };
        }
        if out.main_class.is_empty() {
            out.main_class = parent.main_class.clone();
        }
        inherit!(assets);
        inherit!(asset_index);
        inherit!(downloads);
        inherit!(java_version);
        inherit!(logging);
        inherit!(version_type);
        inherit!(compliance_level);
        inherit!(release_time);
        inherit!(time);
        inherit!(minimum_launcher_version);
        if out.minecraft_arguments.is_none() {
            out.minecraft_arguments = parent.minecraft_arguments.clone();
        }

        // Librerías: hijo primero, y del padre solo las que el hijo no traiga ya.
        let mut libraries = child.libraries.clone();
        for lib in &parent.libraries {
            if !libraries.iter().any(|existing| existing.name == lib.name) {
                libraries.push(lib.clone());
            }
        }
        out.libraries = libraries;

        // Argumentos: padre primero, hijo después (el hijo sobreescribe al final).
        let mut arguments = Arguments::default();
        if let Some(parent_args) = &parent.arguments {
            arguments.game.extend(parent_args.game.clone());
            arguments.jvm.extend(parent_args.jvm.clone());
        }
        if let Some(child_args) = &child.arguments {
            arguments.game.extend(child_args.game.clone());
            arguments.jvm.extend(child_args.jvm.clone());
        }
        out.arguments = Some(arguments);

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rules::Features;

    #[test]
    fn deserializa_argumentos_condicionales() {
        let raw = r#"{
            "id": "1.21.4",
            "mainClass": "net.minecraft.client.main.Main",
            "arguments": {
                "game": [
                    "--username", "${auth_player_name}",
                    {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
                    {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}],
                     "value": ["--width", "${resolution_width}"]}
                ],
                "jvm": ["-Djava.library.path=${natives_directory}"]
            }
        }"#;
        let v = VersionJson::from_str(raw).unwrap();
        let args = v.arguments.as_ref().unwrap();
        // Dos literales y dos condicionales.
        assert_eq!(args.game.len(), 4);

        let env = Environment::current();
        let plain = Features::default();
        // Sin demo ni resolución propia: solo los literales.
        let expanded: Vec<String> = args
            .game
            .iter()
            .flat_map(|a| a.expand(&env, &plain))
            .collect();
        assert_eq!(expanded, vec!["--username", "${auth_player_name}"]);

        let mut custom = Features::default();
        custom.has_custom_resolution = true;
        let expanded: Vec<String> = args
            .game
            .iter()
            .flat_map(|a| a.expand(&env, &custom))
            .collect();
        assert!(expanded.contains(&"--width".to_string()));
        assert!(!expanded.contains(&"--demo".to_string()));
    }

    #[test]
    fn merge_hereda_lo_que_el_hijo_no_trae() {
        let parent = VersionJson::from_str(
            r#"{"id":"1.21.4","mainClass":"net.minecraft.client.main.Main","assets":"19",
                "assetIndex":{"id":"19","url":"http://x/index.json"},
                "downloads":{"client":{"url":"http://x/client.jar","sha1":"aa"}},
                "javaVersion":{"component":"java-runtime-delta","majorVersion":21},
                "libraries":[{"name":"org.lwjgl:lwjgl:3.3.3"},{"name":"solo:del:padre:1"}],
                "arguments":{"jvm":["-cp","${classpath}"],"game":["--username","${auth_player_name}"]}}"#,
        )
        .unwrap();

        let child = VersionJson::from_str(
            r#"{"id":"fabric-loader-0.19.5-1.21.4","inheritsFrom":"1.21.4",
                "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                "libraries":[{"name":"org.lwjgl:lwjgl:3.3.3"},{"name":"net.fabricmc:fabric-loader:0.19.5"}],
                "arguments":{"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main"]}}"#,
        )
        .unwrap();

        let merged = VersionJson::merge(&parent, &child);

        assert_eq!(merged.id, "fabric-loader-0.19.5-1.21.4");
        assert_eq!(merged.inherits_from, None);
        // El mainClass del hijo gana.
        assert!(merged.main_class.contains("KnotClient"));
        // Y lo que el hijo no trae se hereda.
        assert_eq!(merged.assets.as_deref(), Some("19"));
        assert_eq!(merged.java_version.unwrap().major_version, 21);
        assert!(merged.downloads.unwrap().client.is_some());

        // Librerías: del hijo primero, sin duplicar el nombre compartido.
        let names: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "org.lwjgl:lwjgl:3.3.3",
                "net.fabricmc:fabric-loader:0.19.5",
                "solo:del:padre:1"
            ]
        );

        // Argumentos: los del padre primero (ahí va `-cp`), luego los del hijo.
        let args = merged.arguments.unwrap();
        let jvm: Vec<String> = args.jvm.iter().map(|a| match a {
            ArgValue::Plain(s) => s.clone(),
            _ => String::new(),
        }).collect();
        assert_eq!(jvm, vec!["-cp", "${classpath}", "-DFabricMcEmu= net.minecraft.client.main.Main"]);
    }
}
