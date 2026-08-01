use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const DEFAULT_THEME: &str = "catppuccin";
const DEFAULT_LIGHT_THEME: &str = "catppuccin-latte";
const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_THEME_NAME_BYTES: usize = 64;

#[derive(Clone)]
pub(crate) struct ThemeSource {
    is_herdr: bool,
    config_path: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct ThemeDescriptor {
    name: String,
    auto_switch: bool,
    dark_name: Option<String>,
    light_name: Option<String>,
    custom: BTreeMap<String, String>,
}

#[derive(Default, Deserialize)]
struct Config {
    theme: Option<ThemeConfig>,
}

#[derive(Default, Deserialize)]
struct ThemeConfig {
    name: Option<toml::Value>,
    auto_switch: Option<toml::Value>,
    dark_name: Option<toml::Value>,
    light_name: Option<toml::Value>,
    #[serde(default)]
    custom: CustomTheme,
}

#[derive(Default, Deserialize)]
struct CustomTheme {
    accent: Option<toml::Value>,
    panel_bg: Option<toml::Value>,
    surface0: Option<toml::Value>,
    surface1: Option<toml::Value>,
    surface_dim: Option<toml::Value>,
    overlay0: Option<toml::Value>,
    overlay1: Option<toml::Value>,
    text: Option<toml::Value>,
    subtext0: Option<toml::Value>,
    mauve: Option<toml::Value>,
    green: Option<toml::Value>,
    yellow: Option<toml::Value>,
    red: Option<toml::Value>,
    blue: Option<toml::Value>,
    teal: Option<toml::Value>,
    peach: Option<toml::Value>,
}

impl ThemeSource {
    pub(crate) fn from_environment(shell_cmd: &[String]) -> Self {
        Self::from_inputs(
            shell_cmd,
            std::env::var_os("HERDR_CONFIG_PATH"),
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        )
    }

    fn from_inputs(
        shell_cmd: &[String],
        herdr_config_path: Option<OsString>,
        xdg_config_home: Option<OsString>,
        home: Option<OsString>,
    ) -> Self {
        Self {
            is_herdr: command_is_herdr(shell_cmd),
            config_path: config_path(herdr_config_path, xdg_config_home, home),
        }
    }

    pub(crate) fn descriptor(&self) -> Option<ThemeDescriptor> {
        self.is_herdr.then(|| {
            self.config_path
                .as_deref()
                .and_then(read_descriptor)
                .unwrap_or_else(default_descriptor)
        })
    }
}

fn command_is_herdr(shell_cmd: &[String]) -> bool {
    shell_cmd
        .first()
        .and_then(|command| Path::new(command).file_name())
        == Some(OsStr::new("herdr"))
}

fn config_path(
    herdr_config_path: Option<OsString>,
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    herdr_config_path
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            xdg_config_home
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .map(|path| path.join("herdr/config.toml"))
        })
        .or_else(|| {
            home.filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .map(|path| path.join(".config/herdr/config.toml"))
        })
}

fn read_descriptor(path: &Path) -> Option<ThemeDescriptor> {
    let mut file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return None;
    }
    let config: Config = toml::from_str(std::str::from_utf8(&bytes).ok()?).ok()?;
    Some(normalize_descriptor(config.theme.unwrap_or_default()))
}

fn normalize_descriptor(theme: ThemeConfig) -> ThemeDescriptor {
    let mut custom = BTreeMap::new();
    macro_rules! add_color {
        ($field:ident) => {
            if let Some(value) = theme
                .custom
                .$field
                .as_ref()
                .and_then(toml::Value::as_str)
                .and_then(|value| normalize_color(stringify!($field), value))
            {
                custom.insert(stringify!($field).to_string(), value);
            }
        };
    }
    add_color!(accent);
    add_color!(panel_bg);
    add_color!(surface0);
    add_color!(surface1);
    add_color!(surface_dim);
    add_color!(overlay0);
    add_color!(overlay1);
    add_color!(text);
    add_color!(subtext0);
    add_color!(mauve);
    add_color!(green);
    add_color!(yellow);
    add_color!(red);
    add_color!(blue);
    add_color!(teal);
    add_color!(peach);

    let auto_switch = theme
        .auto_switch
        .as_ref()
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    ThemeDescriptor {
        name: theme
            .name
            .as_ref()
            .and_then(toml::Value::as_str)
            .and_then(normalize_name)
            .unwrap_or_else(|| DEFAULT_THEME.to_string()),
        auto_switch,
        dark_name: theme
            .dark_name
            .as_ref()
            .and_then(toml::Value::as_str)
            .and_then(normalize_name)
            .or_else(|| auto_switch.then(|| DEFAULT_THEME.to_string())),
        light_name: theme
            .light_name
            .as_ref()
            .and_then(toml::Value::as_str)
            .and_then(normalize_name)
            .or_else(|| auto_switch.then(|| DEFAULT_LIGHT_THEME.to_string())),
        custom,
    }
}

fn default_descriptor() -> ThemeDescriptor {
    normalize_descriptor(ThemeConfig::default())
}

fn normalize_name(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= MAX_THEME_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then(|| value.to_ascii_lowercase())
}

fn normalize_color(field: &str, value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    if field == "panel_bg" && matches!(value.as_str(), "reset" | "default" | "none" | "transparent")
    {
        return Some("reset".to_string());
    }
    if let Some(hex) = value.strip_prefix('#') {
        return match hex.len() {
            3 if hex.bytes().all(|byte| byte.is_ascii_hexdigit()) => Some(format!(
                "#{}{}{}{}{}{}",
                &hex[0..1],
                &hex[0..1],
                &hex[1..2],
                &hex[1..2],
                &hex[2..3],
                &hex[2..3]
            )),
            6 if hex.bytes().all(|byte| byte.is_ascii_hexdigit()) => Some(format!("#{hex}")),
            _ => None,
        };
    }
    if let Some(inner) = value.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let values = inner
            .split(',')
            .map(str::trim)
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        return (values.len() == 3)
            .then(|| format!("#{:02x}{:02x}{:02x}", values[0], values[1], values[2]));
    }
    let hex = match value.as_str() {
        "black" => "#000000",
        "red" => "#ff0000",
        "green" => "#008000",
        "yellow" => "#ffff00",
        "blue" => "#0000ff",
        "magenta" | "purple" => "#ff00ff",
        "cyan" => "#00ffff",
        "white" => "#ffffff",
        "gray" | "grey" => "#808080",
        _ => return None,
    };
    Some(hex.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct TestFile(PathBuf);

    impl TestFile {
        fn new(contents: &[u8]) -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("mushu-theme-test-{}-{id}", std::process::id()));
            fs::write(&path, contents).expect("write theme test file");
            Self(path)
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn source(command: &str, path: Option<&Path>) -> ThemeSource {
        ThemeSource {
            is_herdr: command_is_herdr(&[command.to_string()]),
            config_path: path.map(Path::to_path_buf),
        }
    }

    fn value(value: &str) -> Option<toml::Value> {
        Some(toml::Value::String(value.to_string()))
    }

    #[test]
    fn recognizes_only_a_herdr_executable_basename() {
        assert!(command_is_herdr(&["herdr".into()]));
        assert!(command_is_herdr(&[
            "/opt/bin/herdr".into(),
            "attach".into()
        ]));
        assert!(!command_is_herdr(&["herdr-wrapper".into()]));
        assert!(!command_is_herdr(&[
            "/bin/sh".into(),
            "-c".into(),
            "herdr".into()
        ]));
    }

    #[test]
    fn config_path_uses_documented_precedence() {
        assert_eq!(
            config_path(
                Some("/explicit.toml".into()),
                Some("/xdg".into()),
                Some("/home".into())
            ),
            Some(PathBuf::from("/explicit.toml"))
        );
        assert_eq!(
            config_path(None, Some("/xdg".into()), Some("/home".into())),
            Some(PathBuf::from("/xdg/herdr/config.toml"))
        );
        assert_eq!(
            config_path(None, None, Some("/home".into())),
            Some(PathBuf::from("/home/.config/herdr/config.toml"))
        );
    }

    #[test]
    fn missing_or_malformed_config_uses_the_default() {
        assert_eq!(
            source("herdr", None).descriptor(),
            Some(default_descriptor())
        );
        let file = TestFile::new(b"[theme\nname = secret");
        assert_eq!(
            source("herdr", Some(&file.0)).descriptor(),
            Some(default_descriptor())
        );
    }

    #[test]
    fn returns_only_normalized_theme_fields_and_custom_tokens() {
        let file = TestFile::new(
            br##"
[unrelated]
command = "do-not-leak"
[theme]
name = " Tokyo-Night "
auto_switch = true
dark_name = "Dracula"
light_name = "bad name!"
[theme.custom]
accent = "#AbC"
panel_bg = "transparent"
text = "rgb(201, 209, 217)"
red = "red"
unknown = "/private/path"
"##,
        );
        let descriptor = source("herdr", Some(&file.0)).descriptor().unwrap();
        assert_eq!(descriptor.name, "tokyo-night");
        assert!(descriptor.auto_switch);
        assert_eq!(descriptor.dark_name.as_deref(), Some("dracula"));
        assert_eq!(descriptor.light_name.as_deref(), Some(DEFAULT_LIGHT_THEME));
        assert_eq!(
            descriptor.custom.get("accent").map(String::as_str),
            Some("#aabbcc")
        );
        assert_eq!(
            descriptor.custom.get("panel_bg").map(String::as_str),
            Some("reset")
        );
        assert_eq!(
            descriptor.custom.get("text").map(String::as_str),
            Some("#c9d1d9")
        );
        assert_eq!(
            descriptor.custom.get("red").map(String::as_str),
            Some("#ff0000")
        );
        let json = serde_json::to_string(&descriptor).unwrap();
        assert!(!json.contains("do-not-leak"));
        assert!(!json.contains("private/path"));
    }

    #[test]
    fn normalizes_all_sixteen_documented_custom_tokens() {
        let custom = CustomTheme {
            accent: value("#123456"),
            panel_bg: value("default"),
            surface0: value("#123"),
            surface1: value("rgb(1, 2, 3)"),
            surface_dim: value("white"),
            overlay0: value("#234567"),
            overlay1: value("#345678"),
            text: value("#456789"),
            subtext0: value("#56789a"),
            mauve: value("purple"),
            green: value("green"),
            yellow: value("yellow"),
            red: value("#654321"),
            blue: value("blue"),
            teal: value("cyan"),
            peach: value("#abcdef"),
        };
        let descriptor = normalize_descriptor(ThemeConfig {
            custom,
            ..ThemeConfig::default()
        });
        assert_eq!(descriptor.custom.len(), 16);
        assert_eq!(
            descriptor.custom.get("surface1").map(String::as_str),
            Some("#010203")
        );
    }

    #[test]
    fn invalid_colors_and_panel_only_reset_values_are_omitted() {
        assert_eq!(normalize_color("accent", "#ggg"), None);
        assert_eq!(normalize_color("accent", "rgb(256, 0, 0)"), None);
        assert_eq!(normalize_color("accent", "reset"), None);
        assert_eq!(
            normalize_color("panel_bg", "none").as_deref(),
            Some("reset")
        );
    }

    #[test]
    fn invalid_field_types_fall_back_without_discarding_valid_fields() {
        let file = TestFile::new(
            br##"
[theme]
name = 42
auto_switch = "yes"
dark_name = ["secret"]
light_name = "one-light"
[theme.custom]
accent = 7
blue = "#123456"
"##,
        );
        let descriptor = source("herdr", Some(&file.0)).descriptor().unwrap();
        assert_eq!(descriptor.name, DEFAULT_THEME);
        assert!(!descriptor.auto_switch);
        assert_eq!(descriptor.dark_name, None);
        assert_eq!(descriptor.light_name.as_deref(), Some("one-light"));
        assert!(!descriptor.custom.contains_key("accent"));
        assert_eq!(
            descriptor.custom.get("blue").map(String::as_str),
            Some("#123456")
        );
    }

    #[test]
    fn auto_switch_uses_herdr_defaults_when_targets_are_omitted() {
        let file = TestFile::new(
            br#"
[theme]
name = "dracula"
auto_switch = true
"#,
        );
        let descriptor = source("herdr", Some(&file.0)).descriptor().unwrap();
        assert_eq!(descriptor.name, "dracula");
        assert_eq!(descriptor.dark_name.as_deref(), Some(DEFAULT_THEME));
        assert_eq!(descriptor.light_name.as_deref(), Some(DEFAULT_LIGHT_THEME));
    }

    #[test]
    fn oversized_config_uses_default_without_parsing_tail_data() {
        let file = TestFile::new(&vec![b'x'; MAX_CONFIG_BYTES as usize + 1]);
        assert_eq!(
            source("herdr", Some(&file.0)).descriptor(),
            Some(default_descriptor())
        );
    }

    #[test]
    fn non_herdr_command_has_no_theme() {
        assert_eq!(source("/bin/sh", None).descriptor(), None);
    }
}
