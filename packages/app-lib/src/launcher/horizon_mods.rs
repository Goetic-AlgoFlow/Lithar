//! Horizon's own mods for Horizon instances, from its registry
//! (docs/api/mod-index.md and docs/mod-browser.md in the Horizon repository).
//!
//! Content mods (`.hmod`, data only) go to `<instance>/horizon-mods/` and the
//! game loads them itself. Native mods (`.hnmod`, Rust source patched into the
//! game) go to `<instance>/horizon-native-mods/`; `horizon::install` compiles
//! the instance's own game with them. Both are turned on or off in the game's
//! `horizon-mods.txt` (`mod.<id>:true`).

use crate::util::fetch::{fetch, fetch_json, write};
use crate::util::io;
use crate::{ErrorKind, State};
use anyhow::{Context, bail};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const INDEX_URL: &str =
    "https://horizon-rust.github.io/horizon-mods/index.json";
const INDEX_SCHEMA: u32 = 1;
const MAX_FILE_SIZE: u64 = 64 << 20;
const MODS_LIST: &str = "horizon-mods.txt";
const INSTALLED_LIST: &str = "horizon-mods-installed.json";

#[derive(Deserialize)]
struct Index {
    schema: u32,
    projects: Vec<Project>,
    #[serde(default)]
    revoked: Vec<Revoked>,
}

#[derive(Deserialize)]
struct Project {
    id: String,
    kind: String,
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    source: Option<String>,
    versions: Vec<ProjectVersion>,
}

#[derive(Deserialize)]
struct ProjectVersion {
    version: String,
    #[serde(default)]
    game: Option<String>,
    file: IndexFile,
}

#[derive(Deserialize)]
struct IndexFile {
    name: String,
    url: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct Revoked {
    id: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    reason: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HorizonModKind {
    /// `.hmod`: blocks, items, recipes and assets, read by the game.
    Mod,
    /// `.hnmod`: native code compiled into the instance's game.
    NativeMod,
}

impl HorizonModKind {
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "mod" => Some(Self::Mod),
            "native-mod" => Some(Self::NativeMod),
            _ => None,
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Self::Mod => "horizon-mods",
            Self::NativeMod => "horizon-native-mods",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Mod => "hmod",
            Self::NativeMod => "hnmod",
        }
    }
}

/// A registry project as the instance sees it.
#[derive(Serialize, Debug)]
pub struct HorizonMod {
    pub id: String,
    pub kind: HorizonModKind,
    pub name: String,
    pub summary: String,
    pub authors: Vec<String>,
    pub icon: Option<String>,
    pub source: Option<String>,
    pub downloads: Option<u64>,
    pub latest_version: String,
    /// The author signs every version (a minisign key in the registry).
    pub signed: bool,
    pub installed_version: Option<String>,
    pub enabled: bool,
    /// The mods it needs on beside it, by id.
    pub requires: Vec<String>,
}

/// What `INSTALLED_LIST` keeps per installed mod.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct InstalledMod {
    pub kind: HorizonModKind,
    pub version: String,
    pub sha256: String,
    /// The mods it needs on beside it (`requires` in its manifest).
    #[serde(default)]
    pub requires: Vec<String>,
}

pub(crate) type InstalledMods = BTreeMap<String, InstalledMod>;

async fn read_index(state: &State) -> crate::Result<Index> {
    let index: Index = fetch_json(
        Method::GET,
        INDEX_URL,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await
    .context("reading Oxyra's mod registry")?;
    if index.schema > INDEX_SCHEMA {
        bail!(ErrorKind::LauncherError(
            "Oxyra's mod registry is newer than this launcher: update it"
                .to_string()
        ));
    }
    Ok(index)
}

pub(crate) async fn installed(instance: &Path) -> InstalledMods {
    match io::read(instance.join(INSTALLED_LIST)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => InstalledMods::new(),
    }
}

async fn save_installed(
    state: &State,
    instance: &Path,
    mods: &InstalledMods,
) -> crate::Result<()> {
    write(
        &instance.join(INSTALLED_LIST),
        &serde_json::to_vec_pretty(mods)?,
        &state.io_semaphore,
    )
    .await
}

pub(crate) fn mod_file(
    instance: &Path,
    id: &str,
    kind: HorizonModKind,
) -> PathBuf {
    instance
        .join(kind.folder())
        .join(format!("{id}.{}", kind.extension()))
}

/// Whether `horizon-mods.txt` turns mod `id` on.
pub(crate) fn is_enabled(list: &str, id: &str) -> bool {
    let key = format!("mod.{id}");
    list.lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(k, v)| k.trim() == key && v.trim() == "true")
}

/// `list` with mod `id` turned on or off, its other lines kept.
fn with_enabled(list: &str, id: &str, on: bool) -> String {
    let key = format!("mod.{id}");
    let mut lines: Vec<&str> = list
        .lines()
        .filter(|line| {
            line.split_once(':').is_none_or(|(k, _)| k.trim() != key)
        })
        .collect();
    let line = format!("{key}:true");
    if on {
        lines.push(&line);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub(crate) async fn mods_list(instance: &Path) -> String {
    io::read(instance.join(MODS_LIST))
        .await
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

/// The registry's mods and native mods, with what this instance has of them.
pub async fn list(instance: &Path) -> crate::Result<Vec<HorizonMod>> {
    let state = State::get().await?;
    let index = read_index(&state).await?;
    let installed = installed(instance).await;
    let enabled = mods_list(instance).await;
    let local: Vec<HorizonMod> =
        local_mods(&installed, &enabled, &index).collect();
    Ok(index
        .projects
        .into_iter()
        .filter_map(|project| {
            let kind = HorizonModKind::parse(&project.kind)?;
            let latest = project.versions.first()?;
            if latest.game.as_deref().is_some_and(|game| {
                game != crate::launcher::horizon::GAME_VERSION
            }) {
                return None;
            }
            let installed_version =
                installed.get(&project.id).map(|m| m.version.clone());
            let requires = installed
                .get(&project.id)
                .map(|m| m.requires.clone())
                .unwrap_or_default();
            Some(HorizonMod {
                enabled: installed_version.is_some()
                    && is_enabled(&enabled, &project.id),
                latest_version: latest.version.clone(),
                signed: project.key.is_some(),
                id: project.id,
                kind,
                name: project.name,
                summary: project.summary,
                authors: project.authors,
                icon: project.icon,
                source: project.source,
                downloads: project.downloads,
                installed_version,
                requires,
            })
        })
        .chain(local)
        .collect())
}

/// The installed mods the registry doesn't list: the player's own, from a file.
fn local_mods<'a>(
    installed: &'a InstalledMods,
    enabled: &'a str,
    index: &'a Index,
) -> impl Iterator<Item = HorizonMod> + 'a {
    installed
        .iter()
        .filter(|(id, _)| !index.projects.iter().any(|p| &p.id == *id))
        .map(|(id, m)| HorizonMod {
            id: id.clone(),
            kind: m.kind,
            name: id.clone(),
            summary: String::new(),
            authors: Vec::new(),
            icon: None,
            source: None,
            downloads: None,
            latest_version: m.version.clone(),
            signed: false,
            installed_version: Some(m.version.clone()),
            enabled: is_enabled(enabled, id),
            requires: m.requires.clone(),
        })
}

/// The registry's file addresses: its own releases, or a GitHub release of the
/// project's repository (docs/api/mod-index.md, `file`).
fn allowed_url(url: &str) -> bool {
    url.strip_prefix("https://github.com/").is_some_and(|rest| {
        let parts: Vec<&str> = rest.split('/').collect();
        parts.len() == 6
            && parts[2] == "releases"
            && parts[3] == "download"
            && parts
                .iter()
                .all(|part| !part.is_empty() && *part != "." && *part != "..")
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Downloads the newest version of registry project `id` into the instance,
/// checked against the registry, and turns it on. A native mod needs
/// `trust_native_code`: it runs with the player's rights once built.
pub async fn install(
    instance: &Path,
    id: &str,
    trust_native_code: bool,
) -> crate::Result<()> {
    let state = State::get().await?;
    let index = read_index(&state).await?;
    let project = index
        .projects
        .iter()
        .find(|project| project.id == id)
        .ok_or(ErrorKind::InputError(format!(
            "{id} is not in Oxyra's mod registry"
        )))?;
    let kind = HorizonModKind::parse(&project.kind).ok_or(
        ErrorKind::InputError(format!(
            "this launcher can't install an Oxyra {} yet",
            project.kind
        )),
    )?;
    if kind == HorizonModKind::NativeMod && !trust_native_code {
        bail!(ErrorKind::InputError(
            "native mods run with full access to this computer: confirm that you trust them first"
                .to_string()
        ));
    }
    let version =
        project
            .versions
            .first()
            .ok_or(ErrorKind::InputError(format!(
                "{id} has no version in Oxyra's mod registry"
            )))?;
    let file = &version.file;
    let sha256 = file.sha256.to_ascii_lowercase();
    if let Some(revoked) = index.revoked.iter().find(|revoked| {
        revoked.id == id
            && revoked.sha256.as_deref().is_none_or(|s| s == sha256)
    }) {
        bail!(ErrorKind::InputError(format!(
            "Oxyra's registry took {id} down: {}",
            revoked.reason
        )));
    }
    if !allowed_url(&file.url) || file.size > MAX_FILE_SIZE {
        bail!(ErrorKind::InputError(format!(
            "{id}'s file is not one the registry may list"
        )));
    }
    // shortcut: the author's minisign signature is not checked, only the
    // registry's pinned SHA-256; check it when a minisign crate is vendored.
    let bytes = fetch(
        &file.url,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    if bytes.len() as u64 != file.size || sha256_hex(&bytes) != sha256 {
        bail!(ErrorKind::LauncherError(format!(
            "{} does not match Oxyra's registry",
            file.name
        )));
    }

    let requires = file_manifest(&bytes, kind)
        .map(|m| m.requires)
        .unwrap_or_default();
    write(&mod_file(instance, id, kind), &bytes, &state.io_semaphore).await?;
    let mut mods = installed(instance).await;
    mods.insert(
        id.to_string(),
        InstalledMod {
            kind,
            version: version.version.clone(),
            sha256,
            requires,
        },
    );
    save_installed(&state, instance, &mods).await?;
    set_enabled(instance, id, true).await
}

/// A mod id that can name its file: lower-case letters, digits, `_` and `-`.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// What a mod file says about itself in its manifest.
#[derive(Debug, PartialEq)]
struct Manifest {
    id: String,
    version: String,
    requires: Vec<String>,
}

/// The manifest of a mod file: `horizon-mod.toml` in a `.hmod`,
/// `horizon-native.json` in a `.hnmod`.
fn file_manifest(
    bytes: &[u8],
    kind: HorizonModKind,
) -> crate::Result<Manifest> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .context("reading the mod file")?;
    let name = match kind {
        HorizonModKind::Mod => "horizon-mod.toml",
        HorizonModKind::NativeMod => "horizon-native.json",
    };
    let mut text = String::new();
    zip.by_name(name)
        .with_context(|| format!("the mod file has no {name}"))?
        .take(1 << 20)
        .read_to_string(&mut text)
        .with_context(|| format!("reading {name}"))?;
    let value: serde_json::Value = match kind {
        HorizonModKind::Mod => toml::from_str::<toml::Table>(&text)
            .ok()
            .and_then(|t| serde_json::to_value(t).ok()),
        HorizonModKind::NativeMod => serde_json::from_str(&text).ok(),
    }
    .unwrap_or_default();
    let text_of = |key: &str| value.get(key)?.as_str().map(str::to_string);
    let id = text_of("id").filter(|id| valid_id(id)).ok_or(
        ErrorKind::InputError(format!("{name} names no valid mod id")),
    )?;
    let requires = value
        .get("requires")
        .and_then(|r| r.as_array())
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str())
                .filter(|id| valid_id(id))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(Manifest {
        id,
        version: text_of("version").unwrap_or_else(|| "local".to_string()),
        requires,
    })
}

/// The mods `id` needs that aren't installed.
fn missing_requirements(mods: &InstalledMods, id: &str) -> Vec<String> {
    mods.get(id)
        .map(|m| {
            m.requires
                .iter()
                .filter(|r| !mods.contains_key(*r))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// The mods turned on in `list` that need `id`.
fn needed_by(mods: &InstalledMods, list: &str, id: &str) -> Vec<String> {
    mods.iter()
        .filter(|(other, m)| {
            m.requires.iter().any(|r| r == id) && is_enabled(list, other)
        })
        .map(|(other, _)| other.clone())
        .collect()
}

/// `id` and every mod it needs, its needs' needs included, once each.
fn with_requirements(mods: &InstalledMods, id: &str) -> Vec<String> {
    let mut out = vec![id.to_string()];
    let mut i = 0;
    while i < out.len() {
        let needs = mods
            .get(&out[i])
            .map(|m| m.requires.clone())
            .unwrap_or_default();
        for r in needs {
            if !out.contains(&r) {
                out.push(r);
            }
        }
        i += 1;
    }
    out
}

/// Installs one of the player's own mods from a `.hmod` or `.hnmod` file and
/// turns it on, returning its id. A native mod needs `trust_native_code`, as
/// one from the registry does.
pub async fn install_file(
    instance: &Path,
    path: &Path,
    trust_native_code: bool,
) -> crate::Result<String> {
    let kind = match path.extension().and_then(|e| e.to_str()) {
        Some("hmod") => HorizonModKind::Mod,
        Some("hnmod") => HorizonModKind::NativeMod,
        _ => bail!(ErrorKind::InputError(
            "an Oxyra mod is a .hmod or .hnmod file".to_string()
        )),
    };
    if kind == HorizonModKind::NativeMod && !trust_native_code {
        bail!(ErrorKind::InputError(
            "native mods run with full access to this computer: confirm that you trust them first"
                .to_string()
        ));
    }
    let state = State::get().await?;
    let bytes = io::read(path).await?;
    if bytes.len() as u64 > MAX_FILE_SIZE {
        bail!(ErrorKind::InputError(
            "the mod file is larger than 64 MiB".to_string()
        ));
    }
    let Manifest {
        id,
        version,
        requires,
    } = file_manifest(&bytes, kind)?;
    write(&mod_file(instance, &id, kind), &bytes, &state.io_semaphore).await?;
    let mut mods = installed(instance).await;
    mods.insert(
        id.clone(),
        InstalledMod {
            kind,
            version,
            sha256: sha256_hex(&bytes),
            requires,
        },
    );
    save_installed(&state, instance, &mods).await?;
    set_enabled(instance, &id, true).await?;
    Ok(id)
}

pub async fn remove(instance: &Path, id: &str) -> crate::Result<()> {
    let state = State::get().await?;
    let mut mods = installed(instance).await;
    refuse_if_needed(&mods, &mods_list(instance).await, id)?;
    if let Some(installed) = mods.remove(id) {
        let path = mod_file(instance, id, installed.kind);
        if path.exists() {
            io::remove_file(&path).await?;
        }
        save_installed(&state, instance, &mods).await?;
    }
    set_enabled(instance, id, false).await
}

fn refuse_if_needed(
    mods: &InstalledMods,
    list: &str,
    id: &str,
) -> crate::Result<()> {
    let users = needed_by(mods, list, id);
    if !users.is_empty() {
        bail!(ErrorKind::InputError(format!(
            "{id} is needed by {}: turn them off first",
            users.join(", ")
        )));
    }
    Ok(())
}

/// Turns mod `id` on, with the mods it needs, or off when no mod on needs it.
pub async fn set_enabled(
    instance: &Path,
    id: &str,
    on: bool,
) -> crate::Result<()> {
    let state = State::get().await?;
    let mods = installed(instance).await;
    let mut list = mods_list(instance).await;
    if on {
        let missing = missing_requirements(&mods, id);
        if !missing.is_empty() {
            bail!(ErrorKind::InputError(format!(
                "{id} needs {}: install it first",
                missing.join(", ")
            )));
        }
        for needed in with_requirements(&mods, id) {
            list = with_enabled(&list, &needed, true);
        }
    } else {
        refuse_if_needed(&mods, &list, id)?;
        list = with_enabled(&list, id, false);
    }
    write(
        &instance.join(MODS_LIST),
        list.as_bytes(),
        &state.io_semaphore,
    )
    .await
}

/// The native mods the instance's game must be built with: installed and on,
/// by id, with the SHA-256 of each file.
pub(crate) async fn enabled_native_mods(
    instance: &Path,
) -> Vec<(String, String)> {
    let list = mods_list(instance).await;
    installed(instance)
        .await
        .into_iter()
        .filter(|(id, m)| {
            m.kind == HorizonModKind::NativeMod
                && is_enabled(&list, id)
                && mod_file(instance, id, m.kind).is_file()
        })
        .map(|(id, m)| (id, m.sha256))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_mods_on_and_off_keeping_other_lines() {
        let list = "mod.zoom:true\nother:value\n";
        let on = with_enabled(list, "minimap", true);
        assert!(is_enabled(&on, "zoom") && is_enabled(&on, "minimap"));
        assert!(on.contains("other:value"));
        let off = with_enabled(&on, "zoom", false);
        assert!(!is_enabled(&off, "zoom") && is_enabled(&off, "minimap"));
        assert!(!is_enabled("mod.zoom:false\n", "zoom"));
    }

    #[test]
    fn accepts_only_github_release_files() {
        assert!(allowed_url(
            "https://github.com/horizon-rust/horizon-mods/releases/download/zoom-2.0.0/zoom-2.0.0.hnmod"
        ));
        assert!(allowed_url(
            "https://github.com/someone/mod/releases/download/v1/mod.hmod"
        ));
        assert!(!allowed_url("http://github.com/a/b/releases/download/t/f"));
        assert!(!allowed_url(
            "https://example.com/a/b/releases/download/t/f"
        ));
        assert!(!allowed_url(
            "https://github.com/a/b/releases/download/../f.hmod"
        ));
        assert!(!allowed_url("https://github.com/a/b/raw/main/f.hmod"));
    }

    fn zipped(name: &str, text: &str) -> Vec<u8> {
        use std::io::Write;
        let mut out = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut out);
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
        zip.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn a_mod_file_names_itself_in_its_manifest() {
        let native = zipped(
            "horizon-native.json",
            r#"{"id": "coords_hud", "version": "2.0.0", "requires": ["oxyra_api", "../x"]}"#,
        );
        assert_eq!(
            file_manifest(&native, HorizonModKind::NativeMod).unwrap(),
            Manifest {
                id: "coords_hud".to_string(),
                version: "2.0.0".to_string(),
                requires: vec!["oxyra_api".to_string()],
            }
        );
        let content =
            zipped("horizon-mod.toml", "id = \"compass_bar\"\nname = \"x\"\n");
        assert_eq!(
            file_manifest(&content, HorizonModKind::Mod).unwrap(),
            Manifest {
                id: "compass_bar".to_string(),
                version: "local".to_string(),
                requires: Vec::new(),
            }
        );
        // An id is a file name in the instance: nothing that leaves its folder.
        let escaping =
            zipped("horizon-native.json", r#"{"id": "../../evil"}"#);
        assert!(file_manifest(&escaping, HorizonModKind::NativeMod).is_err());
        assert!(file_manifest(&native, HorizonModKind::Mod).is_err());
        assert!(!valid_id("Zoom") && !valid_id("") && valid_id("my-mod_2"));
    }

    #[test]
    fn a_mod_comes_on_with_what_it_needs_and_a_needed_one_stays_on() {
        let module = |requires: &[&str]| InstalledMod {
            kind: HorizonModKind::NativeMod,
            version: "1".to_string(),
            sha256: String::new(),
            requires: requires.iter().map(|r| r.to_string()).collect(),
        };
        let mut mods = InstalledMods::new();
        mods.insert("oxyra_api".to_string(), module(&[]));
        mods.insert("particle_core".to_string(), module(&["oxyra_api"]));
        mods.insert("physics".to_string(), module(&["particle_core", "geckolib"]));
        assert_eq!(
            with_requirements(&mods, "particle_core"),
            ["particle_core", "oxyra_api"]
        );
        assert_eq!(missing_requirements(&mods, "physics"), ["geckolib"]);
        assert!(missing_requirements(&mods, "particle_core").is_empty());
        let list = "mod.oxyra_api:true\nmod.particle_core:true\n";
        assert_eq!(needed_by(&mods, list, "oxyra_api"), ["particle_core"]);
        assert!(needed_by(&mods, "mod.oxyra_api:true\n", "oxyra_api").is_empty());
    }
}
