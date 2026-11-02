use crate::launcher::horizon_mods;

pub use crate::launcher::horizon_mods::{HorizonMod, HorizonModKind};

/// Horizon's registry mods, with what this Horizon instance has of them.
pub async fn horizon_mods(instance_id: &str) -> crate::Result<Vec<HorizonMod>> {
    horizon_mods::list(&super::get_full_path(instance_id).await?).await
}

/// Installs a registry mod into a Horizon instance. Native mods need
/// `trust_native_code`; the instance's game is rebuilt with them before its
/// next launch.
pub async fn horizon_mod_install(
    instance_id: &str,
    mod_id: &str,
    trust_native_code: bool,
) -> crate::Result<()> {
    let instance = super::get_full_path(instance_id).await?;
    horizon_mods::install(&instance, mod_id, trust_native_code).await
}

/// Installs one of the player's own mods from a `.hmod` or `.hnmod` file and
/// returns its id. Native mods need `trust_native_code`.
pub async fn horizon_mod_install_file(
    instance_id: &str,
    path: &std::path::Path,
    trust_native_code: bool,
) -> crate::Result<String> {
    let instance = super::get_full_path(instance_id).await?;
    horizon_mods::install_file(&instance, path, trust_native_code).await
}

pub async fn horizon_mod_remove(
    instance_id: &str,
    mod_id: &str,
) -> crate::Result<()> {
    let instance = super::get_full_path(instance_id).await?;
    horizon_mods::remove(&instance, mod_id).await
}

pub async fn horizon_mod_set_enabled(
    instance_id: &str,
    mod_id: &str,
    enabled: bool,
) -> crate::Result<()> {
    let instance = super::get_full_path(instance_id).await?;
    horizon_mods::set_enabled(&instance, mod_id, enabled).await
}
