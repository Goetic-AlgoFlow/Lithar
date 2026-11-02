<p align="center"><img src="packages/assets/icons/lithar/lithar-logo.svg" alt="Lithar" width="128" /></p>

# Lithar

Lithar is a Minecraft: Java Edition launcher with a clean interface, flexible accounts and its own Rust edition of the game, **Oxyra**.

[Русский](readme/ru_ru/README.md)

## Features

- Instances for every Minecraft version, with Fabric, Quilt, Forge and NeoForge, modpacks and content browsing.
- **Oxyra**: Minecraft: Java Edition 26.3 rewritten in Rust, with its own mod loader (choose **Oxyra** as the loader when you create an instance).
- Microsoft and Ely.by sign-in, and offline accounts.
- Skin editing for Microsoft accounts.
- Optional Liquid Glass effects (Matte, Standard, Transparent) in **Settings → Lithar → Visual**.
- No ads, no usage analytics and no error reporting.
- English and Russian translations for Lithar's own screens.

## Oxyra

When you create an instance, choose **Oxyra** as the loader: its version is 26.3. Lithar downloads Minecraft, its assets and Java, then Oxyra, and prepares Oxyra's files from your copy of Minecraft. **Install from file** adds Oxyra mods. Java mods don't run in Oxyra. Oxyra runs on Windows x64, Linux x86-64 and macOS on Apple Silicon.

## Building from source

Lithar has no published releases yet; build it from this repository. You need Rust 1.95, Node.js 24, pnpm 12.11.2 and Java 21.

```bash
pnpm install
pnpm turbo run build --filter=@modrinth/app-frontend
cd apps/app
pnpm tauri build --no-bundle --config '{"build":{"beforeBuildCommand":""}}'
```

The launcher is written to `target/release/Lithar.exe` (Windows). Run `pnpm tauri build` without `--no-bundle` to make installers.

## License

Lithar is free software under the [GNU General Public License v3.0](LICENSE). Some packages carry their own license (see each package's `LICENSE`). Lithar is built from other open-source projects: see [CREDITS.md](CREDITS.md) and [NOTICE.md](NOTICE.md).

Oxyra mods published by Lithar's author are distributed as compiled files under their own license, found in their folder.

## Notice

Lithar is not an official Minecraft product and is not approved by or associated with Mojang or Microsoft. Please comply with Minecraft's terms and the terms of any account service you use, and own a legitimate copy of Minecraft.
