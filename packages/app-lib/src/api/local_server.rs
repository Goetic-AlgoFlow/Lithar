//! Servers the player hosts on this computer: Paper, Purpur or Oxyra's own, each in a folder of
//! its own under `local-servers/`, from their settings to a running process. Friends join by the
//! address of a Radmin VPN network or of the local network ([`network`]).
use crate::state::State;
use crate::{ErrorKind, launcher_user_agent};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

const SETTINGS: &str = "lithar-server.json";
const JAR: &str = "server.jar";
const KEPT_LINES: usize = 1000;
const OXYRA_VERSION: &str = "26.3";
/// The G1 settings Paper's documentation recommends for its servers and forks (docs.papermc.io/paper/aikars-flags).
const AIKAR_FLAGS: [&str; 18] = [
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=200",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
    "-XX:+AlwaysPreTouch",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:G1HeapWastePercent=5",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:G1RSetUpdatingPauseTimePercent=5",
    "-XX:SurvivorRatio=32",
    "-XX:+PerfDisableSharedMem",
    "-XX:MaxTenuringThreshold=1",
];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerKind {
    Paper,
    Purpur,
    Oxyra,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalServer {
    pub name: String,
    pub kind: ServerKind,
    pub version: String,
    pub port: u16,
    pub memory_mb: u32,
    pub motd: String,
    pub max_players: u32,
    /// Off lets accounts Mojang doesn't know join: the player's choice, for a private network.
    pub online_mode: bool,
    pub whitelist: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct ServerStatus {
    pub running: bool,
    pub lines: Vec<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct NetworkAddresses {
    pub radmin_installed: bool,
    /// This computer's address on Radmin VPN (its networks use 26.0.0.0/8).
    pub radmin: Option<String>,
    pub local: Vec<String>,
}

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Arc<Mutex<VecDeque<String>>>,
}

static RUNNING: LazyLock<tokio::sync::Mutex<HashMap<String, Running>>> =
    LazyLock::new(Default::default);

fn failure(message: impl Into<String>) -> crate::Error {
    ErrorKind::LauncherError(message.into()).into()
}

fn servers_dir(state: &State) -> PathBuf {
    state.directories.config_dir.join("local-servers")
}

/// A folder name: nothing that leaves `local-servers/` or that Windows refuses.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && !name.ends_with(['.', ' '])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        && !matches!(
            name.to_ascii_uppercase().as_str(),
            "CON" | "PRN" | "AUX" | "NUL"
        )
}

fn server_dir(state: &State, name: &str) -> crate::Result<PathBuf> {
    if !valid_name(name) {
        return Err(failure(format!("\"{name}\" is not a valid server name")));
    }
    Ok(servers_dir(state).join(name))
}

async fn fetch_json(url: &str) -> crate::Result<serde_json::Value> {
    Ok(http()?
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

fn http() -> crate::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(launcher_user_agent())
        .build()?)
}

/// The versions `kind` offers, newest first.
pub async fn versions(kind: ServerKind) -> crate::Result<Vec<String>> {
    let strings = |value: &serde_json::Value| -> Vec<String> {
        value
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    match kind {
        ServerKind::Oxyra => Ok(vec![OXYRA_VERSION.to_owned()]),
        ServerKind::Purpur => {
            let body = fetch_json("https://api.purpurmc.org/v2/purpur").await?;
            let mut list = strings(&body["versions"]);
            list.reverse();
            Ok(list)
        }
        ServerKind::Paper => {
            let body = fetch_json("https://fill.papermc.io/v3/projects/paper").await?;
            // By family, oldest family first in the API's order is not promised: sorted here.
            let mut list: Vec<String> = body["versions"]
                .as_object()
                .map(|families| families.values().flat_map(strings).collect())
                .unwrap_or_default();
            list.retain(|v| !v.contains('-'));
            list.sort_by_key(|v| std::cmp::Reverse(version_key(v)));
            Ok(list)
        }
    }
}

/// `1.21.4` as numbers; a pre-release (`-rc1`, `-pre2`) sorts by its numbers, under its release.
fn version_key(version: &str) -> (Vec<u32>, bool) {
    let (numbers, tag) = version
        .split_once('-')
        .map_or((version, false), |(n, _)| (n, true));
    (
        numbers.split('.').filter_map(|n| n.parse().ok()).collect(),
        !tag,
    )
}

/// The Java a Minecraft version's server needs.
fn java_major(version: &str) -> u32 {
    let (numbers, _) = version_key(version);
    match numbers.as_slice() {
        [major, ..] if *major >= 26 => 25,
        [1, minor, patch, ..] if *minor == 20 && *patch >= 5 => 21,
        [1, minor, ..] if *minor >= 21 => 21,
        [1, minor, ..] if *minor >= 18 => 17,
        [1, 17, ..] => 16,
        _ => 8,
    }
}

async fn download_jar(server: &LocalServer, to: &Path) -> crate::Result<()> {
    let (url, sha256) = match server.kind {
        ServerKind::Oxyra => return Ok(()),
        ServerKind::Purpur => (
            format!(
                "https://api.purpurmc.org/v2/purpur/{}/latest/download",
                server.version
            ),
            None,
        ),
        ServerKind::Paper => {
            let body = fetch_json(&format!(
                "https://fill.papermc.io/v3/projects/paper/versions/{}/builds?channel=STABLE",
                server.version
            ))
            .await?;
            let file = &body[0]["downloads"]["server:default"];
            let url = file["url"]
                .as_str()
                .ok_or_else(|| failure("Paper gave no download for this version"))?;
            (
                url.to_owned(),
                file["checksums"]["sha256"].as_str().map(str::to_owned),
            )
        }
    };
    let bytes = http()?
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if let Some(expected) = sha256 {
        let got: String = sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if got != expected {
            return Err(failure("the server download failed its checksum"));
        }
    }
    tokio::fs::write(to, &bytes).await?;
    Ok(())
}

/// `server.properties`: the settings Lithar asks for, over what the file already holds.
fn merged_properties(old: &str, server: &LocalServer) -> String {
    let set = [
        ("server-port", server.port.to_string()),
        ("max-players", server.max_players.to_string()),
        ("online-mode", server.online_mode.to_string()),
        ("white-list", server.whitelist.to_string()),
        (
            "motd",
            server.motd.replace(['\r', '\n'], " ").replace('\\', "\\\\"),
        ),
    ];
    let mut out = String::new();
    for line in old.lines() {
        let key = line.split('=').next().unwrap_or_default().trim();
        if !set.iter().any(|(k, _)| *k == key) {
            out.push_str(line);
            out.push('\n');
        }
    }
    for (key, value) in set {
        out.push_str(&format!("{key}={value}\n"));
    }
    out
}

/// Makes or updates a server: its folder, settings and game files. `accept_eula` is the player's
/// own answer to Minecraft's EULA, without which no server runs.
/// A server already made had it accepted then: its settings change without asking again.
pub async fn save(server: LocalServer, accept_eula: bool) -> crate::Result<()> {
    if server.port < 1024 {
        return Err(failure("choose a port from 1024 up"));
    }
    let state = State::get().await?;
    let dir = server_dir(&state, &server.name)?;
    if !accept_eula && read_settings(&dir).await.is_none() {
        return Err(failure("a server needs Minecraft's EULA accepted"));
    }
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(
        dir.join("eula.txt"),
        "# Accepted in Lithar by the player: https://aka.ms/MinecraftEULA\neula=true\n",
    )
    .await?;
    let properties = dir.join("server.properties");
    let old = tokio::fs::read_to_string(&properties)
        .await
        .unwrap_or_default();
    tokio::fs::write(&properties, merged_properties(&old, &server)).await?;
    let changed = read_settings(&dir)
        .await
        .is_none_or(|old| old.kind != server.kind || old.version != server.version);
    if changed || (server.kind != ServerKind::Oxyra && !dir.join(JAR).is_file()) {
        download_jar(&server, &dir.join(JAR)).await?;
    }
    if server.kind != ServerKind::Oxyra {
        tokio::fs::create_dir_all(dir.join("plugins")).await?;
    }
    tokio::fs::write(dir.join(SETTINGS), serde_json::to_vec_pretty(&server)?).await?;
    Ok(())
}

async fn read_settings(dir: &Path) -> Option<LocalServer> {
    let bytes = tokio::fs::read(dir.join(SETTINGS)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub async fn list() -> crate::Result<Vec<LocalServer>> {
    let state = State::get().await?;
    let mut out = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(servers_dir(&state)).await else {
        return Ok(out);
    };
    while let Some(entry) = entries.next_entry().await? {
        if let Some(server) = read_settings(&entry.path()).await {
            out.push(server);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The folder of a server, for the player to add plugins or look at its files.
pub async fn folder(name: &str) -> crate::Result<PathBuf> {
    server_dir(&*State::get().await?, name)
}

/// The newest Oxyra game Lithar built or downloaded; a native build runs its mods on the server too.
async fn oxyra_program(state: &State) -> crate::Result<PathBuf> {
    let versions = state.directories.metadata_dir().join("horizon").join("versions");
    let mut best: Option<(std::time::SystemTime, bool, PathBuf)> = None;
    let mut entries = tokio::fs::read_dir(&versions)
        .await
        .map_err(|_| failure("start an Oxyra instance once: no Oxyra game is installed yet"))?;
    while let Some(entry) = entries.next_entry().await? {
        let program = entry.path().join(if cfg!(windows) {
            "horizon-client.exe"
        } else {
            "horizon-client"
        });
        if let Ok(meta) = tokio::fs::metadata(&program).await
            && let Ok(modified) = meta.modified()
        {
            let native = entry.file_name().to_string_lossy().starts_with("native-");
            if best
                .as_ref()
                .is_none_or(|(at, was_native, _)| (native, modified) > (*was_native, *at))
            {
                best = Some((modified, native, program));
            }
        }
    }
    best.map(|b| b.2)
        .ok_or_else(|| failure("start an Oxyra instance once: no Oxyra game is installed yet"))
}

/// The Java Lithar already knows for `major`, as an instance's launch finds it; installed only when
/// missing. Installing again would replace the files a running server is using.
async fn java_for(major: u32) -> crate::Result<PathBuf> {
    if let Some(known) = crate::api::jre::get_java_versions().await?.get(&major)
        && Path::new(&known.path).is_file()
    {
        return Ok(PathBuf::from(&known.path));
    }
    let path = crate::api::jre::auto_install_java(major).await?;
    crate::api::jre::set_java_version(crate::api::jre::check_jre(path.clone()).await?).await?;
    Ok(path)
}

/// Reads a server's output to its end. A line that isn't UTF-8 (Windows' code page in a Java
/// error) is kept lossily: stopping there would leave the pipe full and the server blocked.
fn keep_lines(from: impl AsyncRead + Unpin + Send + 'static, lines: Arc<Mutex<VecDeque<String>>>) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(from);
        let mut buf = Vec::new();
        while reader.read_until(b'\n', &mut buf).await.is_ok_and(|n| n > 0) {
            let line = String::from_utf8_lossy(&buf).trim_end().to_owned();
            buf.clear();
            if let Ok(mut lines) = lines.lock() {
                if lines.len() == KEPT_LINES {
                    lines.pop_front();
                }
                lines.push_back(line);
            }
        }
    });
}

pub async fn start(name: &str) -> crate::Result<()> {
    let state = State::get().await?;
    let dir = server_dir(&state, name)?;
    let server = read_settings(&dir)
        .await
        .ok_or_else(|| failure(format!("no server named {name}")))?;
    let mut running = RUNNING.lock().await;
    if let Some(r) = running.get_mut(name) {
        if r.child.try_wait()?.is_none() {
            return Err(failure("this server is already running"));
        }
        running.remove(name);
    }
    let mut command = match server.kind {
        ServerKind::Oxyra => {
            let program = oxyra_program(&state).await?;
            let build = program.parent().map(Path::to_path_buf).unwrap_or_default();
            let resources = crate::launcher::horizon::played_resources(&state, &build).await;
            let mut c = Command::new(&program);
            c.arg("--resources")
                .arg(resources)
                .args(["--dedicated", "--nogui", "--port"])
                .arg(server.port.to_string())
                .args(["--world", "world", "--universe"])
                .arg(&dir)
                .env("HORIZON_HOME", &dir)
                .env("HORIZON_NO_AUDIO", "1")
                .env("HORIZON_BUILD_DIR", build);
            c
        }
        ServerKind::Paper | ServerKind::Purpur => {
            let java = java_for(java_major(&server.version)).await?;
            let mut c = Command::new(java);
            c.arg(format!("-Xmx{}M", server.memory_mb))
                .arg(format!("-Xms{}M", server.memory_mb))
                .args(AIKAR_FLAGS)
                .args(["-Dstdout.encoding=UTF-8", "-Dstderr.encoding=UTF-8"])
                .args(["-jar", JAR, "--nogui"]);
            c
        }
    };
    command
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command
        .spawn()
        .map_err(|e| failure(format!("could not start the server: {e}")))?;
    let lines = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(out) = child.stdout.take() {
        keep_lines(out, lines.clone());
    }
    if let Some(err) = child.stderr.take() {
        keep_lines(err, lines.clone());
    }
    let stdin = child.stdin.take();
    running.insert(
        name.to_owned(),
        Running {
            child,
            stdin,
            lines,
        },
    );
    Ok(())
}

/// A line for the server's console.
pub async fn command(name: &str, line: &str) -> crate::Result<()> {
    let mut running = RUNNING.lock().await;
    let stdin = running
        .get_mut(name)
        .and_then(|r| r.stdin.as_mut())
        .ok_or_else(|| failure("this server is not running"))?;
    stdin.write_all(line.replace(['\r', '\n'], " ").as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

/// Asks the server to save and stop; one still running after 30 s is ended.
pub async fn stop(name: &str) -> crate::Result<()> {
    let mut entry = RUNNING.lock().await.remove(name);
    let Some(r) = entry.as_mut() else {
        return Ok(());
    };
    if let Some(stdin) = r.stdin.as_mut() {
        let _ = stdin.write_all(b"stop\n").await;
        let _ = stdin.flush().await;
    }
    let waited =
        tokio::time::timeout(std::time::Duration::from_secs(30), r.child.wait()).await;
    if waited.is_err() {
        r.child.kill().await?;
    }
    Ok(())
}

/// Saves and stops every server the app started, as it quits: one left behind would keep its port
/// with no console to stop it.
pub async fn stop_all() {
    let names: Vec<String> = RUNNING.lock().await.keys().cloned().collect();
    futures::future::join_all(names.iter().map(|name| stop(name))).await;
}

pub async fn status(name: &str) -> crate::Result<ServerStatus> {
    let mut running = RUNNING.lock().await;
    let Some(r) = running.get_mut(name) else {
        return Ok(ServerStatus::default());
    };
    let alive = r.child.try_wait()?.is_none();
    let lines = r
        .lines
        .lock()
        .map(|l| l.iter().cloned().collect())
        .unwrap_or_default();
    Ok(ServerStatus {
        running: alive,
        lines,
    })
}

/// Deletes a stopped server and its worlds. The app asks the player first.
pub async fn delete(name: &str) -> crate::Result<()> {
    let state = State::get().await?;
    let dir = server_dir(&state, name)?;
    let mut running = RUNNING.lock().await;
    if let Some(r) = running.get_mut(name) {
        if r.child.try_wait()?.is_none() {
            return Err(failure("stop this server first"));
        }
        running.remove(name);
    }
    if dir.join(SETTINGS).is_file() {
        tokio::fs::remove_dir_all(&dir).await?;
    }
    Ok(())
}

fn ipv4_of(line: &str) -> Option<String> {
    let (label, value) = line.split_once(':')?;
    if !label.contains("IPv4") {
        return None;
    }
    let value = value.trim().trim_end_matches("(Preferred)").trim();
    value.parse::<std::net::Ipv4Addr>().ok().map(|a| a.to_string())
}

fn addresses_in(ipconfig: &str) -> (Option<String>, Vec<String>) {
    let mut radmin = None;
    let mut local = Vec::new();
    for ip in ipconfig.lines().filter_map(ipv4_of) {
        if ip.starts_with("26.") {
            radmin = Some(ip);
        } else if !ip.starts_with("127.") {
            local.push(ip);
        }
    }
    (radmin, local)
}

/// The addresses friends can join by.
pub async fn network() -> crate::Result<NetworkAddresses> {
    let mut command = Command::new(if cfg!(windows) { "ipconfig" } else { "true" });
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let output = command.output().await?;
    let (radmin, local) = addresses_in(&String::from_utf8_lossy(&output.stdout));
    let radmin_installed = ["ProgramFiles(x86)", "ProgramFiles"].iter().any(|var| {
        std::env::var_os(var)
            .is_some_and(|dir| Path::new(&dir).join("Radmin VPN").is_dir())
    });
    Ok(NetworkAddresses {
        radmin_installed,
        radmin,
        local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_server_output_is_read_past_a_line_that_is_not_utf8() {
        let lines = Arc::new(Mutex::new(VecDeque::new()));
        keep_lines(&b"first\r\nParam\xe8tre incorrect\nDone (1.2s)!\n"[..], lines.clone());
        for _ in 0..100 {
            if lines.lock().unwrap().len() == 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let lines: Vec<String> = lines.lock().unwrap().iter().cloned().collect();
        assert_eq!(lines, ["first", "Param\u{fffd}tre incorrect", "Done (1.2s)!"]);
    }

    #[test]
    fn local_server_names_stay_in_their_folder() {
        assert!(valid_name("Survie entre amis-2"));
        for bad in ["", "..", "a/b", "a\\b", "CON", "nom.", "a:b"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn local_server_properties_keep_what_they_dont_set() {
        let server = LocalServer {
            name: "s".into(),
            kind: ServerKind::Paper,
            version: "1.21.4".into(),
            port: 25570,
            memory_mb: 2048,
            motd: "a\nb".into(),
            max_players: 8,
            online_mode: false,
            whitelist: true,
        };
        let out = merged_properties("level-seed=7\nserver-port=1\n", &server);
        assert!(out.contains("level-seed=7\n") && out.contains("server-port=25570\n"));
        assert!(!out.contains("server-port=1\n") && out.contains("motd=a b\n"));
        assert!(out.contains("online-mode=false\n") && out.contains("white-list=true\n"));
    }

    #[test]
    fn local_server_java_follows_the_version() {
        for (version, java) in [("26.3", 25), ("1.21.4", 21), ("1.20.6", 21), ("1.20.4", 17), ("1.16.5", 8)] {
            assert_eq!(java_major(version), java, "{version}");
        }
    }

    #[test]
    fn local_server_finds_radmin_in_ipconfig() {
        let text = "Carte Ethernet Radmin VPN :\n   Adresse IPv4. . . . . . . . . . . . . .: 26.12.3.4\n\
                    Carte Ethernet Ethernet :\n   Adresse IPv4. . . . . . . . . . . . . .: 192.168.1.20(Preferred)\n";
        let (radmin, local) = addresses_in(text);
        assert_eq!(radmin.as_deref(), Some("26.12.3.4"));
        assert_eq!(local, ["192.168.1.20"]);
    }
}
