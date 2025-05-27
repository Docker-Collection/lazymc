use std::env;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use clap::ArgMatches;
use serde::Deserialize;
use version_compare::Cmp;

use crate::proto;
use crate::util::error::{quit_error, quit_error_msg, ErrorHintsBuilder};
use crate::util::serde::to_socket_addrs;

/// Default configuration file location.
pub const CONFIG_FILE: &str = "lazymc.toml";

/// Configuration version user should be using, or warning will be shown.
const CONFIG_VERSION: &str = "0.2.8";

/// Load config from file or environment variables, based on CLI arguments.
///
/// Quits with an error message on failure.
pub fn load(matches: &ArgMatches) -> Config {
    // Get config path, attempt to canonicalize
    let mut path = PathBuf::from(matches.get_one::<String>("config").unwrap());
    if let Ok(p) = path.canonicalize() {
        path = p;
    }

    // Check if configuration file exists
    if path.is_file() {
        // Load from file
        match Config::load_from_file(path) {
            Ok(config) => config,
            Err(err) => {
                quit_error(
                    anyhow::anyhow!(err).context("Failed to load config"),
                    ErrorHintsBuilder::default()
                        .config(true)
                        .config_test(true)
                        .build()
                        .unwrap(),
                );
            }
        }
    } else {
        // Load from environment variables with defaults
        info!(target: "lazymc::config", "Config file not found at {}, using environment variables and defaults", path.display());
        Config::load_from_env()
    }
}

/// Get environment variable as string with optional default, processing escape sequences
fn get_env_string(key: &str, default: Option<&str>) -> Option<String> {
    let value = env::var(key).ok().or_else(|| default.map(|s| s.to_string()))?;
    Some(process_escape_sequences(&value))
}

/// Process common escape sequences in strings
fn process_escape_sequences(input: &str) -> String {
    input
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .replace("\\\\", "\\")
}

/// Get environment variable as socket address with default
fn get_env_socket_addr(key: &str, default: &str) -> SocketAddr {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| default.parse().unwrap())
}

/// Get environment variable as u32 with default
fn get_env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Get environment variable as u16 with default
fn get_env_u16(key: &str, default: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Get environment variable as bool with default
fn get_env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|s| match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => default,
        })
        .unwrap_or(default)
}

/// Get environment variable as vector of strings
fn get_env_vec_string(key: &str, default: Vec<&str>) -> Vec<String> {
    env::var(key)
        .ok()
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|| default.into_iter().map(|s| s.to_string()).collect())
}

/// Configuration.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Configuration path if known.
    ///
    /// Should be used as base directory for filesystem operations.
    #[serde(skip)]
    pub path: Option<PathBuf>,

    /// Public configuration.
    #[serde(default)]
    pub public: Public,

    /// Server configuration.
    pub server: Server,

    /// Time configuration.
    #[serde(default)]
    pub time: Time,

    /// MOTD configuration.
    #[serde(default)]
    pub motd: Motd,

    /// Join configuration.
    #[serde(default)]
    pub join: Join,

    /// Lockout feature.
    #[serde(default)]
    pub lockout: Lockout,

    /// RCON configuration.
    #[serde(default)]
    pub rcon: Rcon,

    /// Advanced configuration.
    #[serde(default)]
    pub advanced: Advanced,

    /// Config configuration.
    #[serde(default)]
    pub config: ConfigConfig,
}

impl Config {
    /// Load configuration from file.
    pub fn load_from_file(path: PathBuf) -> Result<Self, io::Error> {
        let data = fs::read_to_string(&path)?;
        let mut config: Config = toml::from_str(&data).map_err(io::Error::other)?;

        // Show warning if config version is problematic
        match &config.config.version {
            None => warn!(target: "lazymc::config", "Config version unknown, it may be outdated"),
            Some(version) => match version_compare::compare_to(version, CONFIG_VERSION, Cmp::Ge) {
                Ok(false) => {
                    warn!(target: "lazymc::config", "Config is for older lazymc version, you may need to update it")
                }
                Err(_) => {
                    warn!(target: "lazymc::config", "Config version is invalid, you may need to update it")
                }
                Ok(true) => {}
            },
        }
        config.path.replace(path);

        Ok(config)
    }

    /// Convenience method to load from file path.
    pub fn load(path: PathBuf) -> Result<Self, io::Error> {
        Self::load_from_file(path)
    }

    /// Load configuration from environment variables with defaults.
    pub fn load_from_env() -> Self {
        // Validate required environment variables
        let server_command = env::var("LAZYMC_SERVER_COMMAND")
            .unwrap_or_else(|_| {
                quit_error_msg(
                    "Missing required environment variable: LAZYMC_SERVER_COMMAND".to_string(),
                    ErrorHintsBuilder::default()
                        .build()
                        .unwrap(),
                );
            });

        Self {
            path: None,
            public: Public::from_env(),
            server: Server::from_env(server_command),
            time: Time::from_env(),
            motd: Motd::from_env(),
            join: Join::from_env(),
            lockout: Lockout::from_env(),
            rcon: Rcon::from_env(),
            advanced: Advanced::from_env(),
            config: ConfigConfig::from_env(),
        }
    }
}

/// Public configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Public {
    /// Public address.
    #[serde(deserialize_with = "to_socket_addrs")]
    pub address: SocketAddr,

    /// Minecraft protocol version name hint.
    pub version: String,

    /// Minecraft protocol version hint.
    pub protocol: u32,
}

impl Public {
    fn from_env() -> Self {
        Self {
            address: get_env_socket_addr("LAZYMC_PUBLIC_ADDRESS", "0.0.0.0:25565"),
            version: get_env_string("LAZYMC_PUBLIC_VERSION", Some(proto::PROTO_DEFAULT_VERSION))
                .unwrap_or_else(|| proto::PROTO_DEFAULT_VERSION.to_string()),
            protocol: get_env_u32("LAZYMC_PUBLIC_PROTOCOL", proto::PROTO_DEFAULT_PROTOCOL),
        }
    }
}

impl Default for Public {
    fn default() -> Self {
        Self {
            address: "0.0.0.0:25565".parse().unwrap(),
            version: proto::PROTO_DEFAULT_VERSION.to_string(),
            protocol: proto::PROTO_DEFAULT_PROTOCOL,
        }
    }
}

/// Server configuration.
#[derive(Debug, Deserialize)]
pub struct Server {
    /// Server directory.
    ///
    /// Private because you should use `Server::server_directory()` instead.
    #[serde(default = "option_pathbuf_dot")]
    directory: Option<PathBuf>,

    /// Start command.
    pub command: String,

    /// Server address.
    #[serde(
        deserialize_with = "to_socket_addrs",
        default = "server_address_default"
    )]
    pub address: SocketAddr,

    /// Freeze the server process instead of restarting it when no players online, making it start up faster.
    /// Only works on Unix (Linux or MacOS)
    #[serde(default = "bool_true")]
    pub freeze_process: bool,

    /// Immediately wake server when starting lazymc.
    #[serde(default)]
    pub wake_on_start: bool,

    /// Immediately wake server after crash.
    #[serde(default)]
    pub wake_on_crash: bool,

    /// Probe required server details when starting lazymc, wakes server on start.
    #[serde(default)]
    pub probe_on_start: bool,

    /// Whether this server runs forge.
    #[serde(default)]
    pub forge: bool,

    /// Server starting timeout. Force kill server process if it takes longer.
    #[serde(default = "u32_300")]
    pub start_timeout: u32,

    /// Server stopping timeout. Force kill server process if it takes longer.
    #[serde(default = "u32_150")]
    pub stop_timeout: u32,

    /// To wake server, user must be in server whitelist if enabled on server.
    #[serde(default = "bool_true")]
    pub wake_whitelist: bool,

    /// Block banned IPs as listed in banned-ips.json in server directory.
    #[serde(default = "bool_true")]
    pub block_banned_ips: bool,

    /// Drop connections from banned IPs.
    #[serde(default)]
    pub drop_banned_ips: bool,

    /// Add HAProxy v2 header to proxied connections.
    #[serde(default)]
    pub send_proxy_v2: bool,
}

impl Server {
    fn from_env(command: String) -> Self {
        let directory = get_env_string("LAZYMC_SERVER_DIRECTORY", Some("."))
            .map(PathBuf::from);

        Self {
            directory,
            command,
            address: get_env_socket_addr("LAZYMC_SERVER_ADDRESS", "127.0.0.1:25566"),
            freeze_process: get_env_bool("LAZYMC_SERVER_FREEZE_PROCESS", true),
            wake_on_start: get_env_bool("LAZYMC_SERVER_WAKE_ON_START", false),
            wake_on_crash: get_env_bool("LAZYMC_SERVER_WAKE_ON_CRASH", false),
            probe_on_start: get_env_bool("LAZYMC_SERVER_PROBE_ON_START", false),
            forge: get_env_bool("LAZYMC_SERVER_FORGE", false),
            start_timeout: get_env_u32("LAZYMC_SERVER_START_TIMEOUT", 300),
            stop_timeout: get_env_u32("LAZYMC_SERVER_STOP_TIMEOUT", 150),
            wake_whitelist: get_env_bool("LAZYMC_SERVER_WAKE_WHITELIST", true),
            block_banned_ips: get_env_bool("LAZYMC_SERVER_BLOCK_BANNED_IPS", true),
            drop_banned_ips: get_env_bool("LAZYMC_SERVER_DROP_BANNED_IPS", false),
            send_proxy_v2: get_env_bool("LAZYMC_SERVER_SEND_PROXY_V2", false),
        }
    }

    /// Get the server directory.
    ///
    /// This does not check whether it exists.
    pub fn server_directory(config: &Config) -> Option<PathBuf> {
        // Get directory, relative to config directory if known
        match config.path.as_ref().and_then(|p| p.parent()) {
            Some(config_dir) => Some(config_dir.join(config.server.directory.as_ref()?)),
            None => config.server.directory.clone(),
        }
    }
}

/// Time configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Time {
    /// Sleep after number of seconds.
    pub sleep_after: u32,

    /// Minimum time in seconds to stay online when server is started.
    #[serde(default, alias = "minimum_online_time")]
    pub min_online_time: u32,
}

impl Time {
    fn from_env() -> Self {
        Self {
            sleep_after: get_env_u32("LAZYMC_TIME_SLEEP_AFTER", 60),
            min_online_time: get_env_u32("LAZYMC_TIME_MIN_ONLINE_TIME", 60),
        }
    }
}

impl Default for Time {
    fn default() -> Self {
        Self {
            sleep_after: 60,
            min_online_time: 60,
        }
    }
}

/// MOTD configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Motd {
    /// MOTD when server is sleeping.
    pub sleeping: String,

    /// MOTD when server is starting.
    pub starting: String,

    /// MOTD when server is stopping.
    pub stopping: String,

    /// Use MOTD from Minecraft server once known.
    pub from_server: bool,
}

impl Motd {
    fn from_env() -> Self {
        Self {
            sleeping: get_env_string("LAZYMC_MOTD_SLEEPING", 
                Some("☠ Server is sleeping\n§2☻ Join to start it up"))
                .unwrap(),
            starting: get_env_string("LAZYMC_MOTD_STARTING", 
                Some("§2☻ Server is starting...\n§7⌛ Please wait..."))
                .unwrap(),
            stopping: get_env_string("LAZYMC_MOTD_STOPPING", 
                Some("☠ Server going to sleep...\n⌛ Please wait..."))
                .unwrap(),
            from_server: get_env_bool("LAZYMC_MOTD_FROM_SERVER", false),
        }
    }
}

impl Default for Motd {
    fn default() -> Self {
        Self {
            sleeping: "☠ Server is sleeping\n§2☻ Join to start it up".into(),
            starting: "§2☻ Server is starting...\n§7⌛ Please wait...".into(),
            stopping: "☠ Server going to sleep...\n⌛ Please wait...".into(),
            from_server: false,
        }
    }
}

/// Join method types.
#[derive(Debug, Deserialize, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// Kick client with message.
    Kick,

    /// Hold client connection until server is ready.
    Hold,

    /// Forward connection to another host.
    Forward,

    /// Keep client in temporary fake lobby until server is ready.
    Lobby,
}

impl std::str::FromStr for Method {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "kick" => Ok(Method::Kick),
            "hold" => Ok(Method::Hold),
            "forward" => Ok(Method::Forward),
            "lobby" => Ok(Method::Lobby),
            _ => Err(format!("Unknown join method: {}", s)),
        }
    }
}

/// Join configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Join {
    /// Join methods.
    pub methods: Vec<Method>,

    /// Join kick configuration.
    #[serde(default)]
    pub kick: JoinKick,

    /// Join hold configuration.
    #[serde(default)]
    pub hold: JoinHold,

    /// Join forward configuration.
    #[serde(default)]
    pub forward: JoinForward,

    /// Join lobby configuration.
    #[serde(default)]
    pub lobby: JoinLobby,
}

impl Join {
    fn from_env() -> Self {
        let methods_str = get_env_vec_string("LAZYMC_JOIN_METHODS", vec!["hold", "kick"]);
        let methods = methods_str.into_iter()
            .filter_map(|s| s.parse().ok())
            .collect();

        Self {
            methods,
            kick: JoinKick::from_env(),
            hold: JoinHold::from_env(),
            forward: JoinForward::from_env(),
            lobby: JoinLobby::from_env(),
        }
    }
}

impl Default for Join {
    fn default() -> Self {
        Self {
            methods: vec![Method::Hold, Method::Kick],
            kick: Default::default(),
            hold: Default::default(),
            forward: Default::default(),
            lobby: Default::default(),
        }
    }
}

/// Join kick configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct JoinKick {
    /// Kick message when server is starting.
    pub starting: String,

    /// Kick message when server is stopping.
    pub stopping: String,
}

impl JoinKick {
    fn from_env() -> Self {
        Self {
            starting: get_env_string("LAZYMC_JOIN_KICK_STARTING", 
                Some("Server is starting... §c♥§r\n\nThis may take some time.\n\nPlease try to reconnect in a minute."))
                .unwrap(),
            stopping: get_env_string("LAZYMC_JOIN_KICK_STOPPING", 
                Some("Server is going to sleep... §7☠§r\n\nPlease try to reconnect in a minute to wake it again."))
                .unwrap(),
        }
    }
}

impl Default for JoinKick {
    fn default() -> Self {
        Self {
            starting: "Server is starting... §c♥§r\n\nThis may take some time.\n\nPlease try to reconnect in a minute.".into(),
            stopping: "Server is going to sleep... §7☠§r\n\nPlease try to reconnect in a minute to wake it again.".into(),
        }
    }
}

/// Join hold configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct JoinHold {
    /// Hold client for number of seconds on connect while server starts.
    pub timeout: u32,
}

impl JoinHold {
    fn from_env() -> Self {
        Self {
            timeout: get_env_u32("LAZYMC_JOIN_HOLD_TIMEOUT", 25),
        }
    }
}

impl Default for JoinHold {
    fn default() -> Self {
        Self { timeout: 25 }
    }
}

/// Join forward configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct JoinForward {
    /// IP and port to forward to.
    #[serde(deserialize_with = "to_socket_addrs")]
    pub address: SocketAddr,

    /// Add HAProxy v2 header to proxied connections.
    #[serde(default)]
    pub send_proxy_v2: bool,
}

impl JoinForward {
    fn from_env() -> Self {
        Self {
            address: get_env_socket_addr("LAZYMC_JOIN_FORWARD_ADDRESS", "127.0.0.1:25565"),
            send_proxy_v2: get_env_bool("LAZYMC_JOIN_FORWARD_SEND_PROXY_V2", false),
        }
    }
}

impl Default for JoinForward {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:25565".parse().unwrap(),
            send_proxy_v2: false,
        }
    }
}

/// Join lobby configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct JoinLobby {
    /// Hold client in lobby for number of seconds on connect while server starts.
    pub timeout: u32,

    /// Message banner in lobby shown to client.
    pub message: String,

    /// Sound effect to play when server is ready.
    pub ready_sound: Option<String>,
}

impl JoinLobby {
    fn from_env() -> Self {
        Self {
            timeout: get_env_u32("LAZYMC_JOIN_LOBBY_TIMEOUT", 10 * 60),
            message: get_env_string("LAZYMC_JOIN_LOBBY_MESSAGE", 
                Some("§2Server is starting\n§7⌛ Please wait..."))
                .unwrap(),
            ready_sound: get_env_string("LAZYMC_JOIN_LOBBY_READY_SOUND", 
                Some("block.note_block.chime")),
        }
    }
}

impl Default for JoinLobby {
    fn default() -> Self {
        Self {
            timeout: 10 * 60,
            message: "§2Server is starting\n§7⌛ Please wait...".into(),
            ready_sound: Some("block.note_block.chime".into()),
        }
    }
}

/// Lockout configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Lockout {
    /// Enable to prevent everybody from connecting through lazymc. Instantly kicks player.
    pub enabled: bool,

    /// Kick players with following message.
    pub message: String,
}

impl Lockout {
    fn from_env() -> Self {
        Self {
            enabled: get_env_bool("LAZYMC_LOCKOUT_ENABLED", false),
            message: get_env_string("LAZYMC_LOCKOUT_MESSAGE", 
                Some("Server is closed §7☠§r\n\nPlease come back another time."))
                .unwrap(),
        }
    }
}

impl Default for Lockout {
    fn default() -> Self {
        Self {
            enabled: false,
            message: "Server is closed §7☠§r\n\nPlease come back another time.".into(),
        }
    }
}

/// RCON configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Rcon {
    /// Enable sleeping server through RCON.
    pub enabled: bool,

    /// Server RCON port.
    pub port: u16,

    /// Server RCON password.
    pub password: String,

    /// Randomize server RCON password on each start.
    pub randomize_password: bool,

    /// Add HAProxy v2 header to RCON connections.
    pub send_proxy_v2: bool,
}

impl Rcon {
    fn from_env() -> Self {
        Self {
            enabled: get_env_bool("LAZYMC_RCON_ENABLED", cfg!(windows)),
            port: get_env_u16("LAZYMC_RCON_PORT", 25575),
            password: get_env_string("LAZYMC_RCON_PASSWORD", Some("")).unwrap(),
            randomize_password: get_env_bool("LAZYMC_RCON_RANDOMIZE_PASSWORD", true),
            send_proxy_v2: get_env_bool("LAZYMC_RCON_SEND_PROXY_V2", false),
        }
    }
}

impl Default for Rcon {
    fn default() -> Self {
        Self {
            enabled: cfg!(windows),
            port: 25575,
            password: "".into(),
            randomize_password: true,
            send_proxy_v2: false,
        }
    }
}

/// Advanced configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Advanced {
    /// Rewrite server.properties.
    pub rewrite_server_properties: bool,
}

impl Advanced {
    fn from_env() -> Self {
        Self {
            rewrite_server_properties: get_env_bool("LAZYMC_ADVANCED_REWRITE_SERVER_PROPERTIES", true),
        }
    }
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            rewrite_server_properties: true,
        }
    }
}

/// Config configuration.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ConfigConfig {
    /// Configuration for lazymc version.
    pub version: Option<String>,
}

impl ConfigConfig {
    fn from_env() -> Self {
        Self {
            version: get_env_string("LAZYMC_CONFIG_VERSION", None),
        }
    }
}

fn option_pathbuf_dot() -> Option<PathBuf> {
    Some(".".into())
}

fn server_address_default() -> SocketAddr {
    "127.0.0.1:25566".parse().unwrap()
}

fn u32_300() -> u32 {
    300
}

fn u32_150() -> u32 {
    150
}

fn bool_true() -> bool {
    true
}