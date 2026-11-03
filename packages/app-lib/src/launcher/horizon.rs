//! Horizon: Minecraft Java Edition rewritten in Rust (github.com/horizon-rust/horizon).
//!
//! A Horizon instance installs vanilla Minecraft like any other instance, then
//! downloads the Horizon build for this platform and runs its `horizon-prepare`
//! on those files (docs/api/launch.md in the Horizon repository). Launching runs
//! `horizon-client` instead of Java.

use crate::event::LoadingBarId;
use crate::event::emit::emit_loading;
use crate::install::{
    InstallPhaseDetails, InstallPhaseId, InstallProgress,
    InstallProgressReporter,
};
use crate::instance::QuickPlayType;
use crate::launcher::horizon_mods;
use crate::launcher::io::IOError;
use crate::server_address::ServerAddress;
use crate::state::{Credentials, JavaVersion, WindowSize};
use crate::util::fetch::{fetch, fetch_json, sha1_file_async, write};
use crate::util::io;
use crate::{ErrorKind, State};
use anyhow::{Context, bail};
use daedalus::minecraft::{DownloadType, VersionInfo};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// The only Minecraft version Horizon plays; its preparer refuses any other.
pub const GAME_VERSION: &str = "26.3";

const RELEASES_URL: &str =
    "https://api.github.com/repos/horizon-rust/horizon/releases?per_page=30";
/// Lithar's own releases, which carry the Oxyra launcher: Horizon's command
/// line with `HORIZON_NATIVE_REPO`, for Oxyra's native hook points.
const LITHAR_RELEASES_URL: &str =
    "https://api.github.com/repos/Goetic-AlgoFlow/Lithar/releases?per_page=30";
/// Oxyra's published source (`owner/name@branch`), which native mods build on
/// when Lithar's own checkout is not on this computer.
const OXYRA_SOURCE: &str = "Goetic-AlgoFlow/Oxyra@oxyra";

/// The loader version of an instance that plays Horizon itself rather than
/// Oxyra: Horizon's own launcher and source build its native mods.
pub const OFFICIAL: &str = "official";

pub fn is_official(loader_version: Option<&str>) -> bool {
    loader_version == Some(OFFICIAL)
}
const MOJANG_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    published_at: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct MojangManifest {
    versions: Vec<MojangVersion>,
}

#[derive(Deserialize)]
struct MojangVersion {
    id: String,
    url: String,
    sha1: String,
}

/// The version JSONs a build's preparer accepts, from its `research-lock.json`.
#[derive(Deserialize)]
struct ResearchLock {
    minecraft: LockedMinecraft,
}

#[derive(Deserialize)]
struct LockedMinecraft {
    manifest_sha1: String,
    #[serde(default)]
    accepted_manifest_sha1: Vec<String>,
}

impl LockedMinecraft {
    /// Mojang's current version JSON when the build knows it, else the one it
    /// pins: Mojang keeps every republish at its own address, and a build
    /// refuses one published after it.
    fn version_json(&self, current: MojangVersion) -> MojangVersion {
        if current.sha1 == self.manifest_sha1
            || self.accepted_manifest_sha1.contains(&current.sha1)
        {
            return current;
        }
        MojangVersion {
            url: format!(
                "https://piston-meta.mojang.com/v1/packages/{}/{}.json",
                self.manifest_sha1, current.id
            ),
            sha1: self.manifest_sha1.clone(),
            id: current.id,
        }
    }
}

/// The release archive Horizon publishes for this computer.
fn platform_archive() -> crate::Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "horizon-windows-x64.zip",
        ("linux", "x86_64") => "horizon-linux-x86_64.tar.gz",
        ("macos", "aarch64") => "horizon-macos-arm64.zip",
        ("macos", "x86_64") => "horizon-macos-x64.zip",
        (os, arch) => bail!(ErrorKind::LauncherError(format!(
            "Oxyra has no build for {os} {arch}"
        ))),
    })
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn horizon_dir(state: &State) -> PathBuf {
    state.directories.metadata_dir().join("horizon")
}

fn current_build_file(state: &State) -> PathBuf {
    horizon_dir(state).join("current")
}

/// The installed build folder that instances play, if any.
async fn current_build(state: &State) -> Option<PathBuf> {
    let tag = io::read(current_build_file(state)).await.ok()?;
    let dir = build_dir(state, String::from_utf8_lossy(&tag).trim());
    dir.join(INSTALLED).is_file().then_some(dir)
}

/// A build is a version folder of the official Horizon launcher's layout
/// (`versions/<tag>/`, complete once `.installed` is written), so its CLI can
/// build native mods from it with `HORIZON_LAUNCHER_HOME` set to `horizon_dir`.
fn build_dir(state: &State, tag: &str) -> PathBuf {
    horizon_dir(state).join("versions").join(tag)
}

const INSTALLED: &str = ".installed";

/// Where `horizon-prepare` writes the resources by default, in every release.
fn resources_dir(build: &Path) -> PathBuf {
    build
        .join(".local")
        .join("horizon-client")
        .join(GAME_VERSION)
}

/// Written once the resources and sounds are prepared from this build.
fn prepared_marker(build: &Path) -> PathBuf {
    build.join(".local").join("rustcraft-prepared")
}

fn java_home(java: &JavaVersion) -> Option<PathBuf> {
    Path::new(&java.path)
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

#[cfg(windows)]
pub(crate) fn hide_console(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn hide_console(_command: &mut Command) {}

/// Points `link` at the folder `target`: a junction on Windows, which needs no
/// administrator rights, a symlink elsewhere.
async fn link_dir(target: &Path, link: &Path) -> crate::Result<()> {
    if link.exists() {
        return Ok(());
    }
    if tokio::fs::symlink_metadata(link).await.is_ok() {
        tokio::fs::remove_dir(link)
            .await
            .map_err(|e| IOError::with_path(e, link))?;
    }
    io::create_dir_all(target).await?;
    if let Some(parent) = link.parent() {
        io::create_dir_all(parent).await?;
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target);
        command.stdout(Stdio::null()).stderr(Stdio::null());
        hide_console(&mut command);
        if !command.status().await?.success() {
            bail!(ErrorKind::LauncherError(format!(
                "could not link {} to {}",
                link.display(),
                target.display()
            )));
        }
    }
    #[cfg(not(windows))]
    tokio::fs::symlink(target, link)
        .await
        .map_err(|e| IOError::with_path(e, link))?;
    Ok(())
}

/// Lays out the launcher's own 26.3 files as the official launcher keeps them,
/// which is what every `horizon-prepare` release reads (`--minecraft-dir`).
/// Only Mojang's version JSON is a file of its own: Horizon pins its SHA-1,
/// and the metadata server's copy is rewritten.
async fn minecraft_dir(state: &State, build: &Path) -> crate::Result<PathBuf> {
    let minecraft = horizon_dir(state).join("minecraft");
    let version_dir = minecraft.join("versions").join(GAME_VERSION);
    download_mojang_version_json(
        state,
        build,
        &version_dir.join(format!("{GAME_VERSION}.json")),
    )
    .await?;

    let client_jar = state
        .directories
        .version_dir(GAME_VERSION)
        .join(format!("{GAME_VERSION}.jar"));
    let linked_jar = version_dir.join(format!("{GAME_VERSION}.jar"));
    if linked_jar.exists() {
        io::remove_file(&linked_jar).await?;
    }
    if std::fs::hard_link(&client_jar, &linked_jar).is_err() {
        io::copy(&client_jar, &linked_jar).await?;
    }

    link_dir(
        &state.directories.libraries_dir(),
        &minecraft.join("libraries"),
    )
    .await?;
    link_dir(&state.directories.assets_dir(), &minecraft.join("assets"))
        .await?;
    Ok(minecraft)
}

/// Installs Horizon on top of the vanilla files `install_minecraft` just
/// downloaded, and prepares its resources from them.
#[allow(clippy::too_many_arguments)]
pub async fn install(
    state: &State,
    instance: &Path,
    version_info: &VersionInfo,
    java: &JavaVersion,
    repairing: bool,
    loading_bar: Option<&LoadingBarId>,
    reporter: Option<&InstallProgressReporter>,
    phase_details: &InstallPhaseDetails,
    official: bool,
) -> crate::Result<()> {
    if version_info.id != GAME_VERSION {
        bail!(ErrorKind::LauncherError(format!(
            "Oxyra only plays Minecraft {GAME_VERSION}, not {}",
            version_info.id
        )));
    }
    let report = Progress {
        loading_bar,
        reporter,
        phase_details,
    };

    report.report(0.0, "Downloading Oxyra").await?;
    let build = match download_build(state).await {
        Ok(build) => build,
        Err(error) => match current_build(state).await {
            Some(build) => {
                tracing::warn!(
                    "Could not check for a newer Oxyra build, keeping {}: {error}",
                    build.display()
                );
                build
            }
            None => return Err(error),
        },
    };

    let marker = prepared_marker(&build);
    if repairing || !marker.is_file() || !resources_dir(&build).is_dir() {
        prepare(state, version_info, java, &build, &report).await?;
        write(&marker, b"", &state.io_semaphore).await?;
    }
    let play = local_overlay(state, &build).await?.unwrap_or(build.clone());
    finish(state, &play).await?;
    build_native(state, instance, &build, &report, official).await?;
    report.report(1.0, "Oxyra is ready").await
}

/// The folder of a local Horizon build's programs (a checkout's
/// `target/release` or `target/fast`), played instead of the release's.
const LOCAL_BINARIES: &str = "RUSTCRAFT_HORIZON_BINARIES";
/// In a local overlay: the release version whose resources it plays with.
const OVERLAY_BASE: &str = "rustcraft-base";

/// With `RUSTCRAFT_HORIZON_BINARIES` set, a version `<base>-local-<hash>`
/// holding that build's `horizon-client` (and `horizon-ctl`), played with
/// `base`'s prepared resources: the release keeps the Java exporters and the
/// preparation, which a fix to the game's Rust leaves unchanged.
async fn local_overlay(
    state: &State,
    base: &Path,
) -> crate::Result<Option<PathBuf>> {
    let Some(binaries) = std::env::var_os(LOCAL_BINARIES).map(PathBuf::from)
    else {
        return Ok(None);
    };
    let client = io::read(binaries.join(exe("horizon-client"))).await?;
    let hash: String = sha2::Sha256::digest(&client)
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let base_name = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let overlay = build_dir(state, &format!("{base_name}-local-{hash}"));
    if !overlay.join(INSTALLED).is_file() {
        write(
            &overlay.join(exe("horizon-client")),
            &client,
            &state.io_semaphore,
        )
        .await?;
        let ctl = binaries.join(exe("horizon-ctl"));
        if ctl.is_file() {
            io::copy(&ctl, overlay.join(exe("horizon-ctl"))).await?;
        }
        write(
            &overlay.join(OVERLAY_BASE),
            base_name.as_bytes(),
            &state.io_semaphore,
        )
        .await?;
        write(&overlay.join(INSTALLED), b"", &state.io_semaphore).await?;
    }
    Ok(Some(overlay))
}

/// The resources version folder `dir` plays with: its own, or a local
/// overlay's base's.
pub(crate) async fn played_resources(state: &State, dir: &Path) -> PathBuf {
    match io::read(dir.join(OVERLAY_BASE)).await {
        Ok(base) => resources_dir(&build_dir(
            state,
            String::from_utf8_lossy(&base).trim(),
        )),
        Err(_) => resources_dir(dir),
    }
}

/// Reports Horizon's install steps as the `RunningLoaderProcessors` phase.
struct Progress<'a> {
    loading_bar: Option<&'a LoadingBarId>,
    reporter: Option<&'a InstallProgressReporter>,
    phase_details: &'a InstallPhaseDetails,
}

impl Progress<'_> {
    async fn report(&self, fraction: f64, message: &str) -> crate::Result<()> {
        if let Some(loading_bar) = self.loading_bar {
            emit_loading(loading_bar, 0.0, Some(message))?;
        }
        if let Some(reporter) = self.reporter {
            reporter
                .update(
                    InstallPhaseId::RunningLoaderProcessors,
                    Some(InstallProgress {
                        current: (fraction.clamp(0.0, 1.0) * 1000.0) as u64,
                        total: 1000,
                        secondary: None,
                    }),
                    self.phase_details.clone(),
                )
                .await?;
        }
        Ok(())
    }
}

/// Prepares `build`'s resources and sounds from the launcher's own files.
async fn prepare(
    state: &State,
    version_info: &VersionInfo,
    java: &JavaVersion,
    build: &Path,
    report: &Progress<'_>,
) -> crate::Result<()> {
    let marker = prepared_marker(build);
    if marker.exists() {
        io::remove_file(&marker).await?;
    }

    report
        .report(0.02, "Downloading Minecraft files for Oxyra")
        .await?;
    let minecraft = minecraft_dir(state, build).await?;
    let server = version_info.downloads.get(&DownloadType::Server).ok_or(
        ErrorKind::LauncherError(format!(
            "Minecraft {GAME_VERSION} has no server download"
        )),
    )?;
    let server_jar = minecraft.join(format!("server-{GAME_VERSION}.jar"));
    download_checked(state, &server.url, &server.sha1, &server_jar).await?;

    report.report(0.05, "Preparing Oxyra").await?;
    run_prepare(
        build,
        java,
        [
            "--prepare".as_ref(),
            "--server-jar".as_ref(),
            server_jar.as_os_str(),
            "--minecraft-dir".as_ref(),
            minecraft.as_os_str(),
        ],
    )
    .await?;
    report
        .report(0.75, "Copying Minecraft sounds for Oxyra")
        .await?;
    run_prepare(
        build,
        java,
        [
            "sounds".as_ref(),
            "--minecraft-dir".as_ref(),
            minecraft.as_os_str(),
        ],
    )
    .await
}

/// Which native mods an instance's own game was built with, and as which
/// version folder (`native-<commit>-<key>`, made by the official launcher).
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct NativeBuild {
    key: String,
    version: String,
}

const NATIVE_BUILD: &str = ".rustcraft-native-build.json";

/// Names a set of native mods by their ids and files.
fn native_key(mods: &[(String, String)]) -> String {
    let text: String = mods
        .iter()
        .map(|(id, sha256)| format!("{id} {sha256}\n"))
        .collect();
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn read_native_build(instance: &Path) -> Option<NativeBuild> {
    let bytes = io::read(instance.join(NATIVE_BUILD)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The instance's native build, when it has native mods on and their build is
/// still the current one; `Err` when they need a (re)build.
async fn native_build_dir(
    state: &State,
    instance: &Path,
) -> Result<Option<PathBuf>, ()> {
    let mods = horizon_mods::enabled_native_mods(instance).await;
    if mods.is_empty() {
        return Ok(None);
    }
    match read_native_build(instance).await {
        Some(built) if built.key == native_key(&mods) => {
            let dir = build_dir(state, &built.version);
            if dir.join(INSTALLED).is_file() {
                Ok(Some(dir))
            } else {
                Err(())
            }
        }
        _ => Err(()),
    }
}

/// Whether the instance's native mods changed since its game was built.
pub async fn native_build_stale(state: &State, instance: &Path) -> bool {
    native_build_dir(state, instance).await.is_err()
}

/// Compiles the instance's own game with its native mods turned on, with the
/// official Horizon launcher's `native-build` (docs/native-mods.md there): it
/// downloads its pinned Rust toolchain once, then the source of the commit
/// the mods pin, applies them and compiles. The player confirmed they trust
/// each native mod when installing it (`horizon_mods::install`).
async fn build_native(
    state: &State,
    instance: &Path,
    base: &Path,
    report: &Progress<'_>,
    official: bool,
) -> crate::Result<()> {
    let mods = horizon_mods::enabled_native_mods(instance).await;
    if mods.is_empty() {
        if instance.join(NATIVE_BUILD).exists() {
            io::remove_file(instance.join(NATIVE_BUILD)).await?;
        }
        return Ok(());
    }
    if native_build_dir(state, instance).await.is_ok() {
        return Ok(());
    }

    report
        .report(0.8, "Downloading the Oxyra launcher for native mods")
        .await?;
    let cli = launcher_cli(state, official).await?;
    let key = native_key(&mods);
    let staging = horizon_dir(state).join("native-mods").join(&key[..16]);
    if staging.exists() {
        io::remove_dir_all(&staging).await?;
    }
    for (id, _) in &mods {
        let bytes = io::read(horizon_mods::mod_file(
            instance,
            id,
            horizon_mods::HorizonModKind::NativeMod,
        ))
        .await?;
        let into = staging.join(id);
        tokio::task::spawn_blocking(move || unpack(&bytes, true, false, &into))
            .await??;
    }

    let mut step = String::from("Building Oxyra with native mods");
    report.report(NATIVE_BUILD_START, &step).await?;
    let expected = last_native_build_seconds(state).await;
    let started = std::time::Instant::now();
    let base_version = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut command = Command::new(&cli);
    command
        .arg("native-build")
        .arg(&staging)
        .args(["--version", base_version])
        .args(["--trust-native-code", "--allow-version-mismatch"])
        .env("HORIZON_LAUNCHER_HOME", horizon_dir(state))
        .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
        .current_dir(horizon_dir(state))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // The Oxyra checkout the mods build on, as Lithar was built with (a local build's
    // default); one set when it runs reaches the launcher as it is. Without one on this
    // computer, Oxyra's published source.
    let checkout = (!official)
        .then(|| std::env::var_os("HORIZON_NATIVE_SOURCE"))
        .flatten()
        .or_else(|| option_env!("HORIZON_NATIVE_SOURCE").map(Into::into))
        .map(PathBuf::from)
        .filter(|dir| dir.join(".git").exists());
    match checkout {
        Some(dir) => {
            command.env("HORIZON_NATIVE_SOURCE", dir);
        }
        None if !official
            && std::env::var_os("HORIZON_NATIVE_REPO").is_none() =>
        {
            command.env("HORIZON_NATIVE_REPO", OXYRA_SOURCE);
        }
        None => {}
    }
    hide_console(&mut command);
    let mut child = command.spawn().map_err(|e| IOError::with_path(e, &cli))?;
    let stdout = child.stdout.take().map(|stdout| {
        tokio::spawn(async move {
            let mut text = String::new();
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                text.push_str(&line);
                text.push('\n');
            }
            text
        })
    });
    let mut stderr_text = String::new();
    if let Some(stderr) = child.stderr.take() {
        let mut lines = BufReader::new(stderr).lines();
        let mut tick = tokio::time::interval(NATIVE_BUILD_TICK);
        let fraction =
            || native_build_fraction(started.elapsed().as_secs_f64(), expected);
        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Some(line) = line? else { break };
                    tracing::info!("horizon native-build: {line}");
                    if let Some((_, said)) = line.split_once("] ") {
                        step = format!("Native mods: {said}");
                        report.report(fraction(), &step).await?;
                    }
                    stderr_text.push_str(&line);
                    stderr_text.push('\n');
                }
                _ = tick.tick() => report.report(fraction(), &step).await?,
            }
        }
    }
    let status = child.wait().await?;
    let stdout = match stdout {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    let version = built_version(&stdout).filter(|_| status.success()).ok_or(
        ErrorKind::LauncherError(format!(
            "Oxyra could not build the native mods: {}",
            last_error_line(&stderr_text)
        )),
    )?;
    if !build_dir(state, &version).join(INSTALLED).is_file() {
        bail!(ErrorKind::LauncherError(format!(
            "the native build {version} was not installed"
        )));
    }
    let _ = io::remove_dir_all(&staging).await;
    let _ = write(
        &horizon_dir(state).join(NATIVE_BUILD_SECONDS),
        started.elapsed().as_secs().to_string().as_bytes(),
        &state.io_semaphore,
    )
    .await;
    write(
        &instance.join(NATIVE_BUILD),
        &serde_json::to_vec(&NativeBuild { key, version })?,
        &state.io_semaphore,
    )
    .await
}

/// How long the last native build took, in seconds, beside the launcher's
/// files: the next build's progress and remaining time are measured against it.
const NATIVE_BUILD_SECONDS: &str = "native-build-seconds";
/// A first build downloads Rust and compiles everything: 20 to 30 minutes.
const FIRST_NATIVE_BUILD_SECONDS: f64 = 1500.0;
/// Where the native build starts on the install's progress bar, and how often
/// its progress moves while the compiler prints nothing.
const NATIVE_BUILD_START: f64 = 0.82;
const NATIVE_BUILD_TICK: std::time::Duration =
    std::time::Duration::from_secs(2);

async fn last_native_build_seconds(state: &State) -> f64 {
    io::read(horizon_dir(state).join(NATIVE_BUILD_SECONDS))
        .await
        .ok()
        .and_then(|bytes| String::from_utf8_lossy(&bytes).trim().parse().ok())
        .filter(|seconds: &f64| *seconds >= 1.0)
        .unwrap_or(FIRST_NATIVE_BUILD_SECONDS)
}

/// The progress of a native build `elapsed` seconds in, when it should take
/// `expected`: steady over the time so the download manager can tell the time
/// left, and held just short of the end while a build runs longer.
fn native_build_fraction(elapsed: f64, expected: f64) -> f64 {
    let done = (elapsed / expected).clamp(0.0, 1.0);
    (NATIVE_BUILD_START + (1.0 - NATIVE_BUILD_START) * done).min(0.995)
}

/// The version `native-build` printed: `<version> built and selected ...`.
fn built_version(stdout: &str) -> Option<String> {
    stdout.lines().rev().find_map(|line| {
        line.contains(" built and selected")
            .then(|| line.split_whitespace().next().map(str::to_string))
            .flatten()
    })
}

/// The official Horizon launcher's archive for this computer.
fn launcher_archive() -> crate::Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "horizon-launcher-windows-x64.zip",
        ("linux", "x86_64") => "horizon-launcher-linux-x86_64.tar.gz",
        ("macos", "aarch64") => "horizon-launcher-macos-arm64.zip",
        ("macos", "x86_64") => "horizon-launcher-macos-x64.zip",
        (os, arch) => bail!(ErrorKind::LauncherError(format!(
            "Oxyra builds no native mods on {os} {arch}"
        ))),
    })
}

/// A local build of the official Horizon launcher (a checkout's
/// `launcher/target/.../horizon-launcher`), used instead of the release's.
const LOCAL_LAUNCHER: &str = "RUSTCRAFT_HORIZON_LAUNCHER";

/// The official Horizon launcher's program, downloaded once per release into
/// `launcher/<tag>/`: only its command line is used, for native builds.
async fn launcher_cli(state: &State, official: bool) -> crate::Result<PathBuf> {
    // Set when Lithar is built (a local build's default), else when it runs.
    if let Some(local) = std::env::var_os(LOCAL_LAUNCHER)
        .or_else(|| option_env!("RUSTCRAFT_HORIZON_LAUNCHER").map(Into::into))
        .map(PathBuf::from)
        .filter(|_| !official)
        && local.is_file()
    {
        return Ok(local);
    }
    let url = if official {
        RELEASES_URL
    } else {
        LITHAR_RELEASES_URL
    };
    let releases = list_releases(state, url).await?;
    let archive = launcher_archive()?;
    let release = releases
        .into_iter()
        .find(|r| {
            (!official || r.tag_name.starts_with("launcher@"))
                && r.assets.iter().any(|a| a.name == archive)
        })
        .ok_or(ErrorKind::LauncherError(
            "no Oxyra launcher release found".to_string(),
        ))?;
    let tag = release.tag_name.replace('@', "-");
    let tag = if official {
        tag
    } else {
        format!("lithar-{tag}")
    };
    let dir = horizon_dir(state).join("launcher").join(&tag);
    if !dir.join(INSTALLED).is_file() {
        download_release(state, &release, archive, &dir, false).await?;
    }
    find_file(&dir, &exe("horizon-launcher")).ok_or(
        ErrorKind::LauncherError(format!(
            "the Oxyra launcher {tag} has no {}",
            exe("horizon-launcher")
        ))
        .into(),
    )
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

/// Runs the build's `horizon-prepare` with the launcher's Java 25 first in
/// line (`JAVA_HOME`, then the `PATH`).
async fn run_prepare<const N: usize>(
    build: &Path,
    java: &JavaVersion,
    args: [&std::ffi::OsStr; N],
) -> crate::Result<()> {
    let program = build.join(exe("horizon-prepare"));
    let mut command = Command::new(&program);
    command
        .args(args)
        .arg("--root")
        .arg(build)
        .current_dir(build)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = java_home(java) {
        let mut path = vec![home.join("bin")];
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command.env("JAVA_HOME", &home);
        if let Ok(path) = std::env::join_paths(path) {
            command.env("PATH", path);
        }
    }
    hide_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|e| IOError::with_path(e, &program))?;
    let stderr = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            let mut text = String::new();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                text.push_str(&line);
                text.push('\n');
            }
            text
        })
    });
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            tracing::info!("horizon-prepare: {line}");
        }
    }
    let status = child.wait().await?;
    let stderr = match stderr {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    if !status.success() {
        bail!(ErrorKind::LauncherError(format!(
            "Oxyra could not prepare its resources: {}",
            last_error_line(&stderr)
        )));
    }
    Ok(())
}

/// The `error: ...` line `horizon-prepare` ends on, else all of stderr.
fn last_error_line(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .unwrap_or(stderr.trim())
        .to_string()
}

async fn finish(state: &State, build: &Path) -> crate::Result<()> {
    let tag = build
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    write(
        &current_build_file(state),
        tag.as_bytes(),
        &state.io_semaphore,
    )
    .await
}

/// Downloads the latest Horizon release for this platform, unless it is
/// installed already, and returns its folder.
async fn download_build(state: &State) -> crate::Result<PathBuf> {
    let releases = list_releases(state, RELEASES_URL).await?;
    let archive_name = platform_archive()?;
    let release = pick_release(releases, archive_name).ok_or(
        ErrorKind::LauncherError(format!(
            "no Oxyra release has {archive_name}"
        )),
    )?;
    let version = version_name(&release);
    if version.is_empty()
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
        || version.contains("..")
    {
        bail!(ErrorKind::LauncherError(format!(
            "unexpected Oxyra release {}",
            release.tag_name
        )));
    }

    let build = build_dir(state, &version);
    if !build.join(INSTALLED).is_file() {
        download_release(state, &release, archive_name, &build, true).await?;
    }
    Ok(build)
}

async fn list_releases(
    state: &State,
    url: &str,
) -> crate::Result<Vec<Release>> {
    fetch_json(
        Method::GET,
        url,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await
    .context("listing Oxyra releases")
}

/// Downloads `release`'s `archive_name`, checked against the release's
/// `SHA256SUMS`, and unpacks it into `into`, marked complete with `.installed`.
async fn download_release(
    state: &State,
    release: &Release,
    archive_name: &str,
    into: &Path,
    strip_top: bool,
) -> crate::Result<()> {
    let asset_url = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
            .ok_or(ErrorKind::LauncherError(format!(
                "Oxyra {} has no {name}",
                release.tag_name
            )))
    };
    let sums = fetch(
        &asset_url("SHA256SUMS")?,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    let expected =
        expected_sha256(&String::from_utf8_lossy(&sums), archive_name).ok_or(
            ErrorKind::LauncherError(format!(
                "SHA256SUMS of Oxyra {} does not list {archive_name}",
                release.tag_name
            )),
        )?;
    let archive = fetch(
        &asset_url(archive_name)?,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    let actual: String = sha2::Sha256::digest(&archive)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        bail!(ErrorKind::LauncherError(format!(
            "{archive_name} does not match its SHA-256 (expected {expected}, got {actual})"
        )));
    }

    // Unpacked in place: Windows refuses to rename a folder while an antivirus
    // scans the programs just written into it.
    if into.exists() {
        io::remove_dir_all(into).await?;
    }
    io::create_dir_all(into).await?;
    let unpack_to = into.to_path_buf();
    let is_zip = archive_name.ends_with(".zip");
    tokio::task::spawn_blocking(move || {
        unpack(&archive, is_zip, strip_top, &unpack_to)
    })
    .await??;
    write(&into.join(INSTALLED), b"", &state.io_semaphore).await
}

/// The build to play: the newest Nightly (the current `nightly` or a dated
/// `nightly-<date>`), the channel the official launcher plays and native mods
/// pin, else the newest release (`v<version>`). The launcher's own releases are
/// skipped. GitHub lists releases newest first.
fn pick_release(releases: Vec<Release>, archive: &str) -> Option<Release> {
    let has_archive =
        |r: &Release| r.assets.iter().any(|asset| asset.name == archive);
    let mut releases: Vec<Release> =
        releases.into_iter().filter(has_archive).collect();
    let nightly = releases.iter().position(|r| {
        r.tag_name == "nightly" || r.tag_name.starts_with("nightly-")
    });
    let stable = releases.iter().position(|r| r.tag_name.starts_with('v'));
    Some(releases.swap_remove(nightly.or(stable)?))
}

/// The version folder the official launcher installs a release as
/// (`nightly::version_of` there): `nightly-<date>-<commit>` from a Nightly's
/// name `Nightly <date> · <commit>`, else the tag.
fn version_name(release: &Release) -> String {
    if !release.tag_name.starts_with("nightly") {
        return release.tag_name.clone();
    }
    let mut words = release
        .name
        .strip_prefix("Nightly ")
        .unwrap_or_default()
        .split(" · ");
    let date = words
        .next()
        .filter(|d| d.len() == 10)
        .map(str::to_string)
        .unwrap_or_else(|| release.published_at.chars().take(10).collect());
    match words
        .next()
        .filter(|c| !c.is_empty() && c.chars().all(|c| c.is_ascii_hexdigit()))
    {
        Some(commit) => format!("nightly-{date}-{commit}"),
        None => format!("nightly-{date}"),
    }
}

/// Extracts an archive into `into`, dropping its single top folder with
/// `strip_top`. Entries that would land outside `into` are skipped.
fn unpack(
    archive: &[u8],
    is_zip: bool,
    strip_top: bool,
    into: &Path,
) -> crate::Result<()> {
    let strip = |path: &Path| -> Option<PathBuf> {
        let mut parts = path.components();
        if strip_top {
            parts.next()?;
        }
        let rest = parts.as_path();
        let safe = rest
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
        (safe && !rest.as_os_str().is_empty()).then(|| into.join(rest))
    };
    if is_zip {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive))
            .context("reading the Oxyra archive")?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let Some(path) = entry.enclosed_name().and_then(|p| strip(&p))
            else {
                continue;
            };
            if entry.is_dir() {
                std::fs::create_dir_all(&path)
                    .map_err(|e| IOError::with_path(e, &path))?;
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| IOError::with_path(e, parent))?;
            }
            let mut file = std::fs::File::create(&path)
                .map_err(|e| IOError::with_path(e, &path))?;
            std::io::copy(&mut entry, &mut file)
                .map_err(|e| IOError::with_path(e, &path))?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    &path,
                    std::fs::Permissions::from_mode(mode),
                )
                .map_err(|e| IOError::with_path(e, &path))?;
            }
        }
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        for entry in tar.entries().context("reading the Oxyra archive")? {
            let mut entry = entry?;
            let Some(path) = strip(&entry.path()?) else {
                continue;
            };
            if !path.starts_with(into) {
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| IOError::with_path(e, parent))?;
            }
            entry
                .unpack(&path)
                .map_err(|e| IOError::with_path(e, &path))?;
        }
    }
    Ok(())
}

/// The SHA-256 a `sha256sum`-style listing gives `name`.
fn expected_sha256(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (sum, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name)
            .then(|| sum.to_ascii_lowercase())
    })
}

/// Mojang's own version JSON: Horizon pins its SHA-1, and the copy the
/// metadata server serves is rewritten. When Mojang republished it after
/// `build` was made, the one `build` pins.
async fn download_mojang_version_json(
    state: &State,
    build: &Path,
    destination: &Path,
) -> crate::Result<()> {
    let manifest: MojangManifest = fetch_json(
        Method::GET,
        MOJANG_MANIFEST_URL,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await
    .context("reading Mojang's version manifest")?;
    let version = manifest
        .versions
        .into_iter()
        .find(|version| version.id == GAME_VERSION)
        .ok_or(ErrorKind::LauncherError(format!(
            "Mojang's version manifest has no {GAME_VERSION}"
        )))?;
    let lock = io::read(build.join("research-lock.json"))
        .await
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ResearchLock>(&bytes).ok());
    let version = match lock {
        Some(lock) => lock.minecraft.version_json(version),
        None => version,
    };
    download_checked(state, &version.url, &version.sha1, destination).await
}

async fn download_checked(
    state: &State,
    url: &str,
    sha1: &str,
    destination: &Path,
) -> crate::Result<()> {
    if destination.is_file()
        && sha1_file_async(destination)
            .await
            .is_ok_and(|(_, sum)| sum == sha1)
    {
        return Ok(());
    }
    let bytes = fetch(
        url,
        Some(sha1),
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    write(destination, &bytes, &state.io_semaphore).await
}

/// The version folder the instance plays: its own native build when it has
/// native mods on, else the shared build.
async fn play_dir(state: &State, instance: &Path) -> crate::Result<PathBuf> {
    let missing = || {
        ErrorKind::LauncherError(
            "Oxyra is not installed for this instance; repair it to install it"
                .to_string(),
        )
    };
    match native_build_dir(state, instance).await {
        Ok(Some(native)) => Ok(native),
        Ok(None) => current_build(state)
            .await
            .ok_or_else(missing)
            .map_err(Into::into),
        Err(()) => Err(missing().into()),
    }
}

/// The `horizon-client` the instance plays.
pub async fn client_exe(
    state: &State,
    instance: &Path,
) -> crate::Result<PathBuf> {
    Ok(play_dir(state, instance).await?.join(exe("horizon-client")))
}

/// `horizon-client`'s arguments (docs/api/launch-reference.md), only the ones
/// every build since 0.2.97 takes: the game folder is the working directory
/// and logs go to `HORIZON_HOME`. The session file goes in `scratch`, which
/// lives as long as the game.
pub async fn client_args(
    state: &State,
    instance: &Path,
    credentials: &Credentials,
    resolution: WindowSize,
    quick_play: &QuickPlayType,
    scratch: &Path,
) -> crate::Result<Vec<String>> {
    let build = play_dir(state, instance).await?;
    let session = scratch.join("horizon-session.json");
    write(
        &session,
        session_json(credentials).to_string().as_bytes(),
        &state.io_semaphore,
    )
    .await?;

    let path = |p: &Path| p.to_string_lossy().into_owned();
    let mut args = vec![
        "--resources".to_string(),
        path(&played_resources(state, &build).await),
        "--width".to_string(),
        resolution.0.to_string(),
        "--height".to_string(),
        resolution.1.to_string(),
        "--session-file".to_string(),
        path(&session),
    ];
    match quick_play {
        QuickPlayType::Singleplayer(world) => {
            args.push("--quickPlaySingleplayer".to_string());
            args.push(world.clone());
        }
        QuickPlayType::Server(address) => {
            args.push("--quickPlayMultiplayer".to_string());
            args.push(match address {
                ServerAddress::Unresolved(address) => address.clone(),
                ServerAddress::Resolved {
                    original_host,
                    original_port,
                    ..
                } => format!("{original_host}:{original_port}"),
            });
        }
        QuickPlayType::None => {}
    }
    Ok(args)
}

/// The session JSON of docs/api/launch.md.
fn session_json(credentials: &Credentials) -> serde_json::Value {
    let mut session = serde_json::json!({
        "name": credentials.offline_profile.name,
        "uuid": credentials.offline_profile.id.simple().to_string(),
        "access_token": credentials.access_token,
    });
    if credentials.account_type == "microsoft" {
        session["user_type"] = "msa".into();
    }
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_gets_the_version_json_it_knows() {
        let lock = LockedMinecraft {
            manifest_sha1: "11f1".into(),
            accepted_manifest_sha1: vec!["5df1".into()],
        };
        let current = |sha1: &str| MojangVersion {
            id: "26.3".into(),
            url: format!("https://mojang/{sha1}"),
            sha1: sha1.into(),
        };
        assert_eq!(
            lock.version_json(current("5df1")).url,
            "https://mojang/5df1"
        );
        assert_eq!(
            lock.version_json(current("11f1")).url,
            "https://mojang/11f1"
        );
        let pinned = lock.version_json(current("8060"));
        assert_eq!(pinned.sha1, "11f1");
        assert_eq!(
            pinned.url,
            "https://piston-meta.mojang.com/v1/packages/11f1/26.3.json"
        );
    }

    #[test]
    fn a_native_build_moves_with_the_time_it_should_take() {
        assert_eq!(native_build_fraction(0.0, 200.0), NATIVE_BUILD_START);
        let half = native_build_fraction(100.0, 200.0);
        assert!((half - (NATIVE_BUILD_START + 1.0) / 2.0).abs() < 1e-9);
        assert_eq!(native_build_fraction(10_000.0, 200.0), 0.995);
    }

    #[test]
    fn keeps_the_preparers_error_line() {
        assert_eq!(
            last_error_line("Importing\nerror: no Java 25 found\n"),
            "no Java 25 found"
        );
        assert_eq!(last_error_line(" something else \n"), "something else");
    }

    #[test]
    fn names_versions_like_the_official_launcher() {
        let release = |tag: &str, name: &str| Release {
            tag_name: tag.to_string(),
            name: name.to_string(),
            published_at: "2026-10-31T16:58:38Z".to_string(),
            assets: Vec::new(),
        };
        assert_eq!(
            version_name(&release("nightly", "Nightly 2026-10-31 · d91cab6")),
            "nightly-2026-10-31-d91cab6"
        );
        assert_eq!(
            version_name(&release("nightly-2026-10-30", "Nightly")),
            "nightly-2026-10-31"
        );
        assert_eq!(
            version_name(&release("v0.2.97", "Horizon 0.2.97")),
            "v0.2.97"
        );
    }

    #[test]
    fn finds_archive_checksum() {
        let sums = "ABC123  horizon-windows-x64.zip\ndef456 *horizon-linux-x86_64.tar.gz\n";
        assert_eq!(
            expected_sha256(sums, "horizon-windows-x64.zip").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            expected_sha256(sums, "horizon-linux-x86_64.tar.gz").as_deref(),
            Some("def456")
        );
        assert_eq!(expected_sha256(sums, "horizon-macos-arm64.zip"), None);
    }
}
