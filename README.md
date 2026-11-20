# <img src="src-tauri/icons/icon.svg" alt="" width="48" height="48" align="absmiddle"> Diskman

Diskman is a local backup app for **Windows and Linux**. A **backup** is a named list of entries: each entry pairs a folder you want to protect with its **archive** (destination folder) and is synced either as an exact **mirror** or **add only** (nothing in the archive is ever deleted).

- In the backup editor folders are **tiles**: click an empty slot to choose a folder. Drag tiles to move them (Ctrl copies), Ctrl+C / Ctrl+V or the copy button place a copy elsewhere, also in another backup.
- **Preview** shows exactly what a sync would do (new, updated, moved, deleted files) before anything is touched. It also lists what **cannot be transferred** and why: files locked by another program, missing permissions, links and special files, names that differ only in case or that a FAT/exFAT archive does not allow, files too large for a FAT32 archive, EFS-encrypted files on a drive that cannot hold them.
- After a run that did not transfer everything, a notice links to the details; the error status on the backup's card opens them too. On Windows, entries that need **administrator rights** come with a button that restarts Diskman elevated.
- **Run now** starts backups without preview; several runs wait in a queue and run one after another.
- **Schedules**: each backup can run by itself at a time of day or at an interval (clock button on its card). The clock button in the toolbar writes one schedule into several backups at once; nothing stays linked, each can be changed on its own afterwards.
- **Catching up missed runs** (per schedule, off by default): Diskman logs the schedule while it runs. On the next start it lists the backups whose schedule was due while it was not running, how long they are overdue and how many runs were missed, and offers to run them.
- **Start at login** (asked by the Windows installer or on the first start, switchable in the settings): Diskman starts in the background without a window, only with its tray icon, runs the schedules and catches up missed runs right away; a system notification says so.
- Moved and renamed files are detected and moved inside the archive instead of being copied again.
- Closing the window keeps Diskman running in the notification area (Windows) or system tray (Linux), so schedules keep working. Quit it from the tray icon menu. Started with `--background` (as at login) it opens no window until you open it from the tray or start it again.
- The interface is dark by default, whatever the system's light/dark mode; Settings → Appearance switches to light mode.
- The interface is available in English, German, French, Spanish, Italian, Dutch and Portuguese (Settings → Language); by default it follows the system language.

Built with [Tauri 2](https://tauri.app) (Rust backend) and [Svelte 5](https://svelte.dev) with TypeScript (UI).

## Supported systems

| System | Versions | Packages |
|---|---|---|
| Windows | 10, 11 (x64) | NSIS installer (`.exe`), `.msi` |
| Linux | Ubuntu 22.04+, Debian 12+, Fedora 38+, Arch, openSUSE (x64) | `.deb`, `.rpm`, `.AppImage` |

On Linux, the tray icon needs AppIndicator support. GNOME shows tray icons only with an extension (e.g. *AppIndicator and KStatusNotifierItem Support*); without it, starting Diskman again brings the hidden window back.

## Getting started

Requirements on every system: [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 22. On Linux also the system libraries of Tauri (WebKitGTK 4.1, GTK 3, AppIndicator):

```sh
scripts/linux/install-build-deps.sh   # apt, dnf, pacman or zypper; asks for sudo
```

Then, in the project root:

```sh
npm install        # install frontend dependencies
npm run tauri dev  # start the app in development mode with hot reload (short: npm start)
```

Build the installers of the system you are on (each system builds its own; the CI builds all of them):

```sh
npm run tauri build  # short: npm run bundle
```

The output ends up in `src-tauri/target/release/bundle/`:

- Windows: `nsis/Diskman_0.1.0_x64-setup.exe` and `msi/Diskman_0.1.0_x64_en-US.msi`
- Linux: `deb/Diskman_0.1.0_amd64.deb`, `rpm/Diskman-0.1.0-1.x86_64.rpm` and `appimage/Diskman_0.1.0_amd64.AppImage`

Checks (the CI runs the same on Windows and Linux):

```sh
npm run check                  # TypeScript and Svelte types, every language has every text
cd src-tauri
cargo fmt --check              # code style
cargo clippy --all-targets -- -D warnings
cargo test                     # backend tests
```

## Continuous integration and releases

- [`.github/workflows/ci.yml`](.github/workflows/ci.yml) checks every push and pull request on Windows and Linux.
- [`.github/workflows/release.yml`](.github/workflows/release.yml) builds all installers when a version tag is pushed (`git tag v0.1.0 && git push origin v0.1.0`) and attaches them to a draft release. Keep the version equal in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.

## Project structure

Both halves are split into layers; each layer only calls the ones below it. Everything that depends on the operating system lives in a `platform` folder, in the backend and in the UI; see [docs/platforms.md](docs/platforms.md).

```
src/                    Svelte UI (TypeScript)
  main.ts                 mounts App.svelte
  App.svelte              app shell: title bar (Windows), page title, notices, which page is shown
  api/                    the only code that talks to Tauri
    backend.ts              one function per backend command
    types.ts                the data the backend sends and receives (mirrors the Rust types)
    appWindow.ts            window buttons and Tauri runtime facts
  platform/               the system the UI runs on (index.ts) and UI only for one system
    windows/                the title bar drawn by the app (Windows has no native frame)
  stores/                 app state and logic, no markup (*.svelte.ts)
    config / navigation / jobs / checks / analysis / problems / backups / platform / admin / …
  features/               one folder per area of the UI, with its pages and parts
    backups/                the home page: toolbar, backup cards, list view (search, sort)
    editor/                 editing a backup: folder tiles, mode, colour, drag and drop
    analysis/               analysis dialog of a check
    problems/               what the last run did not transfer, and why
    schedule/               schedule dialog of a backup, question about missed runs
    settings/               settings page, one component per tab
    info/                   info page
  components/             UI parts used by several features (logo, dialogs, hints …)
  utils/                  pure helpers: formatting, icons, colours, browser storage …
  i18n/                   translations: one file per language in locales/, en.ts is the reference
                          (types.ts derives the type every language must match)
  styles/global.css       colours, base elements and shared button styles
src-tauri/
  src/                    Rust backend (layers in src/lib.rs)
    main.rs / lib.rs        start; lib.rs builds the app and registers the commands
    commands/               IPC: the commands the UI calls (config, sync, dialog, platform)
    jobs/                   queue, schedules, missed runs, the run of one backup, job progress
    sync/                   sync engine: scanner, planner (what to change), executor, issues
    model/                  the configuration as data, its migration and validation
    storage/                data folder, config.json and the SQLite run history
    platform/               everything OS-specific: facade modules shared by all systems …
      sys/windows/            … and their Windows half
      sys/linux/              … and their Linux half
    state.rs                state shared while the app runs
  capabilities/           permissions of the window (default.json; windows.json only on Windows)
  icons/                  app icons
  tauri.conf.json         window, installer and icon settings shared by all systems
  tauri.windows.conf.json   … what differs on Windows (frameless window, NSIS/MSI)
  tauri.linux.conf.json     … what differs on Linux (deb/rpm/AppImage)
  rustfmt.toml            code style of the backend (`cargo fmt`)
scripts/                  helper scripts (linux/ for Linux only)
docs/                     notes for developers
.github/workflows/        CI and releases for Windows and Linux
```

## Data

Diskman stores its data in one folder:

- Windows: `%APPDATA%\Diskman`
- Linux: `$XDG_DATA_HOME/diskman`, usually `~/.local/share/diskman`

It holds:

- `config.json`: backups, archives, schedules and settings
- `diskman.db`: SQLite history of all runs (also the source of the "last run" status)

Set `DISKMAN_DATA_DIR` to use a different folder. Deleting `diskman.db` (while Diskman is not running) only clears the history; the configuration stays.

## License

Diskman is free software under the [GNU General Public License, version 3 or later](LICENSE) (GPL-3.0-or-later). You may use, change and share the code. If you build it into an app of your own and distribute that app, it must be released under the GPL with its source code too.

The names and logos of Diskman and PigeonBits are not covered by the license, see [NOTICE](NOTICE).
