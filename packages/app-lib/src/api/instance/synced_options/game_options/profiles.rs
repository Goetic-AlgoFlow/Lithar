//! Named options profiles: saved copies of an `options.txt` that can become the shared game
//! settings of every synced instance in one go.
//!
//! A profile keeps the file as Minecraft wrote it and the game version it came from. Applying it
//! seeds the shared settings from it, as choosing a sync source instance does, then writes them to
//! every participating instance. The catalog converts each known setting for each instance's
//! version (a key renamed between versions, a value written differently), and copies other keys,
//! such as mod key bindings, as they are, to the instances whose Minecraft knows them.

use super::api_types::SaveGameSettingsResult;
use super::options_file::{GameOptionsDocument, options_path, read_document};
use super::source_selection::seed_shared_settings;
use super::write_shared_settings::sync_all_participating_instances;
use crate::ErrorKind;
use crate::State;
use crate::util::io;
use anyhow::bail;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const OPTIONS_FILE: &str = "options.txt";
const PROFILE_FILE: &str = "profile.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameOptionsProfile {
    pub id: String,
    pub name: String,
    /// The Minecraft version that wrote the file, which the catalog converts from.
    pub game_version: String,
    pub saved_at: i64,
    /// How many settings the file holds.
    pub settings: u32,
}

fn profiles_dir(state: &State) -> PathBuf {
    super::super::synced_options_path(state).join("profiles")
}

/// A folder name for `name`: its letters and digits, lowercased, words joined by `-`.
fn profile_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            id.extend(c.to_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_end_matches('-').chars().take(64).collect()
}

/// The Minecraft version a file at `path` most likely comes from: the last version-looking part
/// of its path (`profiles/NewCAT 1.21.11 better/options.txt` gives `1.21.11`).
pub fn guess_profile_version(path: &Path) -> Option<String> {
    let text = path.to_string_lossy();
    let mut found = None;
    for word in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        let word = word.trim_matches('.');
        let parts: Vec<&str> = word.split('.').collect();
        if (2..=3).contains(&parts.len())
            && parts.iter().all(|p| !p.is_empty() && p.len() <= 3)
            && super::catalog::release_version(word).is_some()
        {
            found = Some(word.to_string());
        }
    }
    found
}

async fn read_profile(dir: &Path) -> crate::Result<GameOptionsProfile> {
    let bytes = io::read(dir.join(PROFILE_FILE)).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub async fn list_profiles() -> crate::Result<Vec<GameOptionsProfile>> {
    let state = State::get().await?;
    let dir = profiles_dir(&state);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut profiles = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|error| io::IOError::with_path(error, &dir))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| io::IOError::with_path(error, &dir))?
    {
        match read_profile(&entry.path()).await {
            Ok(profile) => profiles.push(profile),
            Err(error) => tracing::warn!(
                "Skipping options profile {}: {error}",
                entry.path().display()
            ),
        }
    }
    profiles.sort_by_key(|profile| profile.name.to_lowercase());
    Ok(profiles)
}

async fn store_profile(
    state: &State,
    name: &str,
    game_version: &str,
    bytes: &[u8],
) -> crate::Result<GameOptionsProfile> {
    let id = profile_id(name);
    if id.is_empty() {
        bail!(ErrorKind::InputError(
            "an options profile needs a name with letters or digits"
                .to_string()
        ));
    }
    if super::catalog::release_version(game_version.trim()).is_none() {
        bail!(ErrorKind::InputError(format!(
            "{game_version} is not a Minecraft release version"
        )));
    }
    let document = GameOptionsDocument::parse(bytes)?;
    let settings = document.effective_entries().len() as u32;
    if settings == 0 {
        bail!(ErrorKind::InputError(
            "this options.txt has no settings".to_string()
        ));
    }
    let profile = GameOptionsProfile {
        id: id.clone(),
        name: name.trim().to_string(),
        game_version: game_version.trim().to_string(),
        saved_at: Utc::now().timestamp(),
        settings,
    };
    let dir = profiles_dir(state).join(&id);
    io::create_dir_all(&dir).await?;
    io::write(dir.join(OPTIONS_FILE), bytes).await?;
    io::write(dir.join(PROFILE_FILE), serde_json::to_vec_pretty(&profile)?)
        .await?;
    Ok(profile)
}

/// Saves `instance_id`'s current `options.txt` as the profile `name`, replacing one of that name.
pub async fn save_profile_from_instance(
    name: &str,
    instance_id: &str,
) -> crate::Result<GameOptionsProfile> {
    let state = State::get().await?;
    let metadata = crate::state::get_instance(instance_id, &state.pool)
        .await?
        .ok_or(ErrorKind::InputError("unknown instance".to_string()))?;
    let path = options_path(&metadata, &state);
    if !path.exists() {
        bail!(ErrorKind::InputError(
            "launch the instance once so Minecraft can create options.txt"
                .to_string(),
        ));
    }
    let (_, bytes) = read_document(&path).await?;
    store_profile(
        &state,
        name,
        &metadata.applied_content_set.game_version,
        &bytes,
    )
    .await
}

/// Saves the `options.txt` at `path`, from another launcher or a backup, as the profile `name`;
/// `game_version` is the Minecraft version that wrote it.
pub async fn import_profile(
    name: &str,
    path: &Path,
    game_version: &str,
) -> crate::Result<GameOptionsProfile> {
    let state = State::get().await?;
    let (_, bytes) = read_document(path).await?;
    store_profile(&state, name, game_version, &bytes).await
}

/// Makes the profile `id` the shared game settings and writes them to every instance that syncs
/// its game settings. Instances that are running get them when they close.
pub async fn apply_profile(id: &str) -> crate::Result<SaveGameSettingsResult> {
    let state = State::get().await?;
    let dir = profiles_dir(&state).join(profile_id(id));
    let profile = read_profile(&dir).await?;
    let (document, _) = read_document(&dir.join(OPTIONS_FILE)).await?;
    let _guard = state.synced_options.lock().await;
    seed_shared_settings(
        &state,
        &document,
        &HashSet::new(),
        &profile.game_version,
        None,
    )
    .await?;
    Ok(sync_all_participating_instances(&state).await)
}

pub async fn delete_profile(id: &str) -> crate::Result<()> {
    let state = State::get().await?;
    let dir = profiles_dir(&state).join(profile_id(id));
    if dir.exists() {
        io::remove_dir_all(&dir).await?;
    }
    Ok(())
}

#[cfg(test)]
mod lithar_options_profile_tests {
    use super::{guess_profile_version, profile_id};
    use std::path::Path;

    #[test]
    fn lithar_profile_ids_are_safe_folder_names() {
        assert_eq!(profile_id("  My PvP  options! "), "my-pvp-options");
        assert_eq!(profile_id("../../etc"), "etc");
        assert_eq!(profile_id("***"), "");
    }

    #[test]
    fn lithar_profile_version_is_guessed_from_the_path() {
        let path = Path::new(
            "C:/Users/a/AppData/Roaming/AstralRinthApp/profiles/NewCAT 1.21.11 better/options.txt",
        );
        assert_eq!(guess_profile_version(path).as_deref(), Some("1.21.11"));
        assert_eq!(
            guess_profile_version(Path::new(
                "D:/Lithar/profiles/Oxyra 26.3/options.txt"
            ))
            .as_deref(),
            Some("26.3")
        );
        assert_eq!(
            guess_profile_version(Path::new("C:/games/options.txt")),
            None
        );
    }
}
