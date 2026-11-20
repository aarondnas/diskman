# Platforms

Diskman supports **Windows** and **Linux**. This note explains where code for only one system goes,
so the shared code stays free of `#[cfg]` checks and `if (windows)` branches.

## The rule

Shared code never asks which system it runs on. It calls a **platform facade**, and the facade has
one implementation per system. A system-only file lives in a folder named after its system.

| Where | Shared (all systems) | Windows only | Linux only |
|---|---|---|---|
| Backend | `src-tauri/src/platform/*.rs` | `src-tauri/src/platform/sys/windows/` | `src-tauri/src/platform/sys/linux/` |
| UI | `src/platform/index.ts` | `src/platform/windows/` | `src/platform/linux/` (not needed yet) |
| Tauri config | `src-tauri/tauri.conf.json` | `src-tauri/tauri.windows.conf.json` | `src-tauri/tauri.linux.conf.json` |
| Installer | — | `src-tauri/windows/installer-hooks.nsh` (NSIS) | — |
| Window permissions | `src-tauri/capabilities/default.json` | `src-tauri/capabilities/windows.json` | — |
| Rust dependencies | `[dependencies]` | `[target.'cfg(windows)'.dependencies]` | `[target.'cfg(target_os = "linux")'.dependencies]` |
| Scripts | `scripts/` | — | `scripts/linux/` |
| Texts | one text | `{ windows: '…', linux: '…' }` in a locale file | ← |

## Backend: facade and `sys`

The same split as in Rust's standard library:

```
platform/
  mod.rs          picks sys/windows or sys/linux (#[path]); other systems are a compile error
  autostart.rs    facade: starting in the background at login
  elevation.rs    facade: administrator rights
  fs.rs           facade: error codes, copying, timestamps, read-only flag, folder listings
  journal.rs      facade: change journal of a volume (NTFS USN, read without admin rights)
  locale.rs       facade: the system's display language (texts shown without a window)
  paths.rs        facade: data folder
  power.rs        facade: no sleep and a question before shutdown while a sync runs
  volume.rs       facade: file system limits of an archive (FAT32, exFAT …)
  window.rs       facade: the main window (created on demand), its frame and icon
  tray.rs         Tauri's tray API; the same code on both systems
  notification.rs Tauri's notification plugin; the same code on both systems
  sys/windows/    autostart.rs elevation.rs fs.rs journal.rs locale.rs paths.rs power.rs volume.rs window.rs (Win32 API via windows-sys)
  sys/linux/      autostart.rs elevation.rs fs.rs journal.rs locale.rs paths.rs power.rs volume.rs window.rs (libc, systemd-inhibit)
```

The facade holds the documentation, the shared types (`VolumeLimits`, `FsError`) and the logic
that is the same everywhere (e.g. which names FAT accepts). The `sys` modules only hold what really
differs. Everything outside `platform/` uses the facade, e.g. `platform::fs::copy_file`, never
`sys` directly (it is private).

**Adding something OS-specific:**

1. Declare the function in the facade module, with its documentation.
2. Implement it in **every** `sys/<os>/` module. A missing one is a compile error on that system.
3. Test shared behaviour in the facade or the caller; test system behaviour with
   `#[cfg(windows)]` / `#[cfg(unix)]` tests next to the code that needs them.

## What differs, and why

| Topic | Windows | Linux |
|---|---|---|
| Window frame | none (`decorations: false`); the UI draws a Windows 11 style title bar | the desktop's native frame |
| Tray | left click opens the window, right click the menu | every click opens the menu ("Open Diskman" is in it); GNOME needs an extension |
| Data folder | `%APPDATA%\Diskman` | `$XDG_DATA_HOME/diskman` |
| Start at login | value `Diskman` in `HKCU\…\CurrentVersion\Run`; the NSIS installer asks (`src-tauri/windows/installer-hooks.nsh`), the MSI does not, so the app asks on its first start | `~/.config/autostart/diskman.desktop` (the AppImage itself when run from one); packages cannot ask per user, so the app asks on its first start |
| Notification | toast in the bottom right corner | the desktop's notification (libnotify; GNOME shows it at the top) |
| Display language without a window | first preferred UI language of Windows | `LC_ALL`, `LC_MESSAGES`, `LANG` |
| Administrator rights | restart with UAC for unreadable files | not offered; permissions are fixed on the files (chmod, chown) |
| Path comparison | case-insensitive | decided by the archive's file system: case-sensitive (ext4 …) or not (FAT, exFAT) |
| Names on the archive | Windows already refuses what FAT refuses | names with `: ? * " < > \|` or a trailing dot or space are listed as "name not allowed" for FAT/exFAT archives |
| Special files | do not exist | named pipes, sockets, devices are listed as skipped |
| Copying | `CopyFileExW` (attributes, alternate data streams) | `std::fs::copy`; on FAT/exFAT without permission support only the content |
| Timestamps | modification and creation time are carried over | modification time (Linux cannot set the creation time) |
| Error codes | Win32 (`ERROR_SHARING_VIOLATION` …) | errno (`EACCES`, `ENOSPC` …) |

## UI

`src/platform/index.ts` exports `OS`, `IS_WINDOWS` and `IS_LINUX`. The Tauri CLI sets
`TAURI_ENV_PLATFORM` while it builds the frontend for a system (passed through in
`vite.config.ts`), so the value is fixed in the bundle of that system.

- A component only for one system goes to `src/platform/<os>/` and is shown with
  `{#if IS_WINDOWS}` in the shared component that uses it (see `App.svelte` and the title bar).
- What the backend knows at run time (administrator rights) comes from `stores/platform.svelte.ts`.
- A text that differs per system is an object in the locale file:
  `tray: { windows: '… notification area …', linux: '… system tray …' }`; `t('info.tray')` picks
  the right one. A language may use one text for both where its wording fits both.

## Checking the other system

The CI ([.github/workflows/ci.yml](../.github/workflows/ci.yml)) builds, lints and tests on
`windows-latest` and `ubuntu-22.04` for every push. Locally, work on the system you are on; for the
other one, rely on the CI or a VM. Linux build requirements: `scripts/linux/install-build-deps.sh`.
