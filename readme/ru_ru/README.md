<p align="center"><img src="../../packages/assets/icons/lithar/lithar-logo.svg" alt="Lithar" width="128" /></p>

# Lithar

Lithar — лаунчер Minecraft: Java Edition с чистым интерфейсом, гибкой работой с аккаунтами и собственной версией игры на Rust — **Oxyra**.

[English](../../README.md)

## Возможности

- Сборки для любой версии Minecraft с Fabric, Quilt, Forge и NeoForge, модпаки и каталог контента.
- **Oxyra**: Minecraft: Java Edition 26.3, переписанный на Rust, со своим загрузчиком модов (выберите загрузчик **Oxyra** при создании сборки).
- Вход через Microsoft и Ely.by, а также офлайн-аккаунты.
- Редактирование скинов для аккаунтов Microsoft.
- Опциональные эффекты Liquid Glass (Matte, Standard, Transparent) в **Настройки → Lithar → Visual**.
- Без рекламы, без аналитики использования и без отправки отчётов об ошибках.
- Английский и русский переводы собственных экранов Lithar.

## Oxyra

При создании сборки выберите загрузчик **Oxyra** (или **Horizon** — сам Horizon из его собственных релизов): её версия — 26.3. Lithar скачивает Minecraft, его ресурсы и Java, затем Oxyra и готовит файлы Oxyra из вашей копии Minecraft. **Установить из файла** добавляет моды Oxyra. Моды для Java в Oxyra не работают. Oxyra работает на Windows x64, Linux x86-64 и macOS на Apple Silicon.

Нативные моды Oxyra компилируются в игру сборки перед её следующим запуском, на исходном коде Oxyra ([Goetic-AlgoFlow/Oxyra](https://github.com/Goetic-AlgoFlow/Oxyra)). Первая сборка загружает Rust и занимает 20–30 минут; следующие проходят быстрее.

## Скачать

Скачайте `Lithar.exe` из [последнего релиза](https://github.com/Goetic-AlgoFlow/Lithar/releases/latest) и запустите его (Windows x64; используется Microsoft Edge WebView2, входящий в Windows 10 и 11).

## Сборка из исходного кода

Чтобы собрать Lithar самостоятельно, нужны Rust 1.95, Node.js 24, pnpm 12.11.2 и Java 21.

```bash
pnpm install
pnpm turbo run build --filter=@modrinth/app-frontend
cd apps/app
pnpm tauri build --no-bundle --config '{"build":{"beforeBuildCommand":""}}'
```

Лаунчер появится в `target/release/Lithar.exe` (Windows). Запустите `pnpm tauri build` без `--no-bundle`, чтобы получить установщики.

## Лицензия

Lithar — свободное программное обеспечение под [GNU General Public License v3.0](../../LICENSE). У некоторых пакетов своя лицензия (см. `LICENSE` в каждом пакете). Lithar создан на основе других проектов с открытым исходным кодом: см. [CREDITS.md](../../CREDITS.md) и [NOTICE.md](../../NOTICE.md).

Моды Oxyra от автора Lithar распространяются в скомпилированном виде под собственной лицензией, которая лежит в их папке.

## Важно

Lithar не является официальным продуктом Minecraft, не одобрен и не связан с Mojang или Microsoft. Соблюдайте условия Minecraft и сервисов аккаунтов, которыми вы пользуетесь, и владейте легальной копией Minecraft.
