# Credits

Lithar is built on these open-source projects. Thank you to their authors and contributors.

| Project | What Lithar uses | License |
|---|---|---|
| [Modrinth App](https://github.com/modrinth/code) | The launcher Lithar is built from: its interface, instance management and game launching. | GPL-3.0 (launcher), LGPL-3.0 (`packages/api-client`), MIT/Apache-2.0 (`packages/async-minecraft-ping`) |
| [AstralRinth](https://git.xorison.dev/didirus/AstralRinth) | The fork of Modrinth App that Lithar continues: external and offline accounts, skin handling and the visual settings. | GPL-3.0 |
| [Horizon](https://github.com/horizon-rust/horizon) | Minecraft: Java Edition rewritten in Rust, the base of Oxyra. | MIT |
| [authlib-injector](https://github.com/yushijinhun/authlib-injector) | Ely.by sign-in in the game. | AGPL-3.0 (downloaded at run time, not bundled) |

The full list of third-party libraries is in `Cargo.lock` and `pnpm-lock.yaml`; each library keeps its own license.

Modrinth's name and logo are trademarks of Rinth, Inc. Lithar is not affiliated with Modrinth, AstralRinth, Horizon, Mojang or Microsoft.
