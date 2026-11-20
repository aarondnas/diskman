//! Updates of Diskman itself, from the releases on GitHub (tauri-plugin-updater). A published
//! release carries a `latest.json` with the new version, its notes and a signed installer per
//! system (.github/workflows/release.yml); the updater only installs what carries the signature of
//! the key in tauri.conf.json (plugins > updater > pubkey).
//!
//! Diskman checks a minute after it starts and then once a day (`Settings::check_updates`), or when
//! asked in the settings. It never installs by itself: the user starts it, and not while a sync
//! runs or waits. Installing ends Diskman; Windows' installer starts it again, elsewhere it
//! restarts itself.
//!
//! If updates can no longer come this way (the signing key or its password was lost, so new
//! releases are signed with a new key, maybe from a new repository), Diskman says so and points to
//! the website, where the new version can be downloaded and installed by hand (`Lost`). Being
//! offline is not such a case.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::utils::platform::bundle_type;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

use crate::platform::{locale, notification};
use crate::state::AppState;

/// Where new versions are downloaded by hand and installed over the old one: for copies that
/// cannot update themselves, and when updates no longer come from GitHub with the known key
/// (`Lost`). Made for users, unlike the release page on GitHub.
pub const WEBSITE_URL: &str = "https://pigeonbits.com/diskman";

/// After this many checks in a row that reached the server but found no release there, the
/// release is taken as gone (one may be a hiccup of GitHub).
const MISSES_UNTIL_LOST: u32 = 2;

/// The first check waits a moment, so the start of Diskman stays quick.
const FIRST_CHECK: Duration = Duration::from_secs(60);
/// How often the timer looks whether the daily check is due (it also notices sleep and the
/// setting being switched on).
const TICK: Duration = Duration::from_secs(60 * 60);
/// Checked again after this long.
const CHECK_EVERY: chrono::Duration = chrono::Duration::hours(24);

/// What the UI shows about updates.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateStatus {
    /// The version running now
    pub current: String,
    /// A newer version on GitHub, if the last check found one
    pub available: Option<Available>,
    pub checked_at: Option<DateTime<Utc>>,
    pub checking: bool,
    /// Downloading and installing; Diskman ends when it is done
    pub installing: bool,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Why the last check or install failed
    pub error: Option<String>,
    /// This installation can update itself: an installed build (NSIS, MSI, AppImage, deb, rpm),
    /// not one started from the sources
    pub can_install: bool,
    /// Updates no longer come this way: the new version has to come from the website
    pub lost: Option<Lost>,
}

/// Why updates no longer come from GitHub with the known key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lost {
    /// GitHub answers, but has no release with a `latest.json` (any more): the repository was
    /// deleted, renamed or moved
    Missing,
    /// The update found is not signed with the key this version knows (the key was replaced),
    /// so it was not installed
    Untrusted,
}

/// The update could not be installed because its signature does not fit the known key (or was
/// tampered with): it will never install, only a download by hand helps.
fn untrusted(e: &Error) -> bool {
    matches!(e, Error::Minisign(_) | Error::SignatureUtf8(_) | Error::Base64(_) | Error::SignedVersionMismatch { .. })
}

#[derive(Debug, Clone, Serialize)]
pub struct Available {
    pub version: String,
    /// The release notes
    pub notes: Option<String>,
}

/// Managed by Tauri: the status, and the update found (needed to install it).
pub struct Updates {
    status: Mutex<UpdateStatus>,
    found: Mutex<Option<Update>>,
    /// Version a notification was shown for (once per version)
    notified: Mutex<Option<String>>,
    /// Checks in a row that found no release on the server (`MISSES_UNTIL_LOST`)
    misses: Mutex<u32>,
    /// Version whose signature did not fit (`Lost::Untrusted`)
    rejected: Mutex<Option<String>>,
}

impl Updates {
    pub fn new(current: String) -> Self {
        let status = UpdateStatus { current, can_install: bundle_type().is_some(), ..Default::default() };
        Updates {
            status: Mutex::new(status),
            found: Mutex::new(None),
            notified: Mutex::new(None),
            misses: Mutex::new(0),
            rejected: Mutex::new(None),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap().clone()
    }
}

/// Asks GitHub for a newer version. Returns the status afterwards (also when it failed: then
/// with `error`). A check already running or an install going on is not disturbed.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let updates = app.state::<Updates>();
    {
        let mut s = updates.status.lock().unwrap();
        if s.checking || s.installing {
            return s.clone();
        }
        s.checking = true;
        s.error = None;
    }
    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let mut s = updates.status.lock().unwrap();
    s.checking = false;
    s.checked_at = Some(Utc::now());
    let mut misses = updates.misses.lock().unwrap();
    match result {
        Ok(found) => {
            *misses = 0;
            s.available = found.as_ref().map(|u| Available { version: u.version.clone(), notes: u.body.clone() });
            // Still the version that was refused: it stays refused; another one gets its chance
            let rejected = updates.rejected.lock().unwrap();
            let refused = found.as_ref().is_some_and(|u| rejected.as_deref() == Some(u.version.as_str()));
            s.lost = refused.then_some(Lost::Untrusted);
            *updates.found.lock().unwrap() = found;
        }
        // The server answered, but without a release (offline is a network error instead)
        Err(Error::ReleaseNotFound) => {
            *misses += 1;
            if *misses >= MISSES_UNTIL_LOST {
                s.lost = Some(Lost::Missing);
            }
            s.error = Some(Error::ReleaseNotFound.to_string());
        }
        Err(e) => s.error = Some(e.to_string()),
    }
    s.clone()
}

/// Downloads and installs the update found by the last check, then ends Diskman (see the module
/// notes). Refused while a sync runs or waits; from the start of the download on no further sync
/// starts and running checks stop. Returns only if it failed: then everything goes on as before.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let state = Arc::clone(app.state::<Arc<AppState>>().inner());
    if !updates.status.lock().unwrap().can_install {
        return Err("This copy of Diskman cannot update itself; download the new version instead.".into());
    }
    let update = updates.found.lock().unwrap().clone().ok_or("No update was found.")?;
    {
        // Lock order: job, then queue
        let job = state.job.lock().unwrap();
        let queue = state.queue.lock().unwrap();
        if job.running || !queue.is_empty() {
            return Err("A backup is running or waiting. Update once it is done.".into());
        }
        state.quitting.store(true, Ordering::SeqCst);
    }
    for check in state.checks.lock().unwrap().values() {
        check.stop.store(true, Ordering::SeqCst);
    }
    {
        let mut s = updates.status.lock().unwrap();
        s.installing = true;
        s.downloaded = 0;
        s.total = None;
        s.error = None;
    }
    let progress = |chunk: usize, total: Option<u64>| {
        let mut s = updates.status.lock().unwrap();
        s.downloaded += chunk as u64;
        s.total = total;
    };
    match update.download_and_install(progress, || {}).await {
        // Windows ended Diskman already when it started the installer
        Ok(()) => app.restart(),
        Err(e) => {
            state.quitting.store(false, Ordering::SeqCst);
            let mut s = updates.status.lock().unwrap();
            s.installing = false;
            s.error = Some(e.to_string());
            if untrusted(&e) {
                // Signed with another key: it would fail every time, the website has it
                s.lost = Some(Lost::Untrusted);
                *updates.rejected.lock().unwrap() = Some(update.version.clone());
            }
            Err(e.to_string())
        }
    }
}

/// The check in the background: a minute after the start, then once a day, while the setting
/// allows it. A new version found this way is announced once with a notification (Diskman often
/// runs without its window).
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            let (enabled, language) = {
                let state = app.state::<Arc<AppState>>();
                let config = state.config.lock().unwrap();
                (config.settings.check_updates, config.settings.language.clone().or_else(locale::system_language))
            };
            let due = app.state::<Updates>().status().checked_at.is_none_or(|at| Utc::now() - at >= CHECK_EVERY);
            if enabled && due {
                let status = check(&app).await;
                let language = language.as_deref().unwrap_or("en");
                // Once per version found, or once that updates no longer come this way
                let (key, body) = match (status.lost, status.available) {
                    (Some(Lost::Missing), _) => ("lost".to_string(), Some(lost_notice(language))),
                    (_, Some(found)) => (found.version.clone(), Some(notice(language, &found.version))),
                    _ => (String::new(), None),
                };
                if let Some(body) = body {
                    let updates = app.state::<Updates>();
                    let mut notified = updates.notified.lock().unwrap();
                    if notified.as_deref() != Some(key.as_str()) {
                        notification::show(&app, "Diskman", &body);
                        *notified = Some(key);
                    }
                }
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// The notification about a new version, in the language of the app.
fn notice(language: &str, version: &str) -> String {
    match language {
        "de" => format!("Diskman {version} ist verfügbar. Du kannst es in Diskman installieren."),
        "fr" => format!("Diskman {version} est disponible. Vous pouvez l’installer depuis Diskman."),
        "es" => format!("Diskman {version} está disponible. Puedes instalarlo desde Diskman."),
        "it" => format!("Diskman {version} è disponibile. Puoi installarlo da Diskman."),
        "nl" => format!("Diskman {version} is beschikbaar. Je kunt het installeren vanuit Diskman."),
        "pt" => format!("O Diskman {version} está disponível. Pode instalá-lo a partir do Diskman."),
        _ => format!("Diskman {version} is available. You can install it from within Diskman."),
    }
}

/// The notification that updates no longer come this way (`Lost::Missing`).
fn lost_notice(language: &str) -> String {
    let site = WEBSITE_URL.trim_start_matches("https://");
    match language {
        "de" => format!("Diskman findet keine Updates mehr. Die neue Version gibt es auf {site}."),
        "fr" => format!("Diskman ne trouve plus de mises à jour. La nouvelle version est sur {site}."),
        "es" => format!("Diskman ya no encuentra actualizaciones. La nueva versión está en {site}."),
        "it" => format!("Diskman non trova più aggiornamenti. La nuova versione è su {site}."),
        "nl" => format!("Diskman vindt geen updates meer. De nieuwe versie staat op {site}."),
        "pt" => format!("O Diskman já não encontra atualizações. A nova versão está em {site}."),
        _ => format!("Diskman no longer finds updates. The new version is on {site}."),
    }
}

#[cfg(test)]
mod tests {
    use super::{lost_notice, notice, untrusted, Error};

    #[test]
    fn the_lost_notice_names_the_website_in_every_language() {
        for lang in ["de", "fr", "es", "it", "nl", "pt", "en", "xx"] {
            assert!(lost_notice(lang).contains("pigeonbits.com"), "{lang}");
        }
    }

    #[test]
    fn only_a_signature_that_does_not_fit_counts_as_untrusted() {
        assert!(untrusted(&Error::SignatureUtf8("x".into())));
        assert!(untrusted(&Error::SignedVersionMismatch { signed: "1.0.0".into(), announced: "2.0.0".into() }));
        // Offline or a missing release are other cases
        assert!(!untrusted(&Error::Network("offline".into())));
        assert!(!untrusted(&Error::ReleaseNotFound));
    }

    #[test]
    fn the_notice_names_the_version_in_every_language() {
        for lang in ["de", "fr", "es", "it", "nl", "pt", "en", "xx"] {
            assert!(notice(lang, "1.2.3").contains("1.2.3"), "{lang}");
        }
    }
}
