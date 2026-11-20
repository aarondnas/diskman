//! Ending Diskman while a sync runs: quitting from the tray asks first, and quitting, the restart
//! with administrator rights and the end of the session stop all work and give the running sync a
//! moment to finish the file it is copying and to record its result.
//!
//! Waiting is a courtesy, not what keeps the archive intact: a copy only replaces a file once it
//! is complete and on the drive, so even ending at once leaves no broken file (`sync::executor`).
//! What the run did not get to, the next run does.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::platform::locale;
use crate::state::AppState;

/// How long quitting waits for the running sync to stop.
pub const QUIT_WAIT: Duration = Duration::from_secs(10);
/// Windows gives an application about 5 s after the session ended.
pub const SESSION_END_WAIT: Duration = Duration::from_secs(4);

/// Stops everything for good: no further run starts, the queue is emptied, the running sync and
/// all checks are cancelled. Waits up to `limit` until the sync has stopped; true if it did.
pub fn stop_all(state: &AppState, limit: Duration) -> bool {
    state.quitting.store(true, Ordering::SeqCst);
    {
        // Lock order: job, then queue
        let _job = state.job.lock().unwrap();
        state.queue.lock().unwrap().clear();
        state.cancel.store(true, Ordering::SeqCst);
    }
    for check in state.checks.lock().unwrap().values() {
        check.stop.store(true, Ordering::SeqCst);
    }
    let until = Instant::now() + limit;
    while state.job.lock().unwrap().running {
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

/// Stops everything (`stop_all`) on a thread of its own, then ends Diskman.
pub fn exit_after_stop(app: AppHandle, limit: Duration) {
    let state = Arc::clone(app.state::<Arc<AppState>>().inner());
    std::thread::spawn(move || {
        if !stop_all(&state, limit) {
            eprintln!("The running sync did not stop in time; Diskman ends anyway");
        }
        app.exit(0);
    });
}

/// "Quit" in the tray: at once when nothing runs, otherwise only after asking.
pub fn request_quit(app: &AppHandle) {
    let state = Arc::clone(app.state::<Arc<AppState>>().inner());
    let language = {
        let config = state.config.lock().unwrap();
        config.settings.language.clone().or_else(locale::system_language)
    };
    if !state.job.lock().unwrap().running {
        exit_after_stop(app.clone(), QUIT_WAIT);
        return;
    }
    let t = texts(language.as_deref().unwrap_or("en"));
    let handle = app.clone();
    app.dialog()
        .message(t.quit_question)
        .title("Diskman")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(t.quit.into(), t.keep.into()))
        .show(move |quit| {
            if quit {
                exit_after_stop(handle, QUIT_WAIT);
            }
        });
}

/// Reason the system shows while a sync holds off sleep and shutdown (`platform::power`).
pub fn busy_reason(state: &AppState) -> String {
    let language = state.config.lock().unwrap().settings.language.clone().or_else(locale::system_language);
    texts(language.as_deref().unwrap_or("en")).busy.to_string()
}

struct Texts {
    quit_question: &'static str,
    quit: &'static str,
    keep: &'static str,
    busy: &'static str,
}

fn texts(language: &str) -> Texts {
    let (quit_question, quit, keep, busy) = match language {
        "de" => (
            "Ein Backup läuft gerade. Trotzdem beenden?\n\nDie Datei, die gerade kopiert wird, wird noch fertig geschrieben; den Rest erledigt der nächste Lauf.",
            "Beenden",
            "Weiterlaufen lassen",
            "Diskman synchronisiert gerade ein Backup.",
        ),
        "fr" => (
            "Une sauvegarde est en cours. Quitter quand même ?\n\nLe fichier en cours de copie est terminé ; la prochaine exécution fera le reste.",
            "Quitter",
            "Continuer",
            "Diskman synchronise une sauvegarde.",
        ),
        "es" => (
            "Hay una copia en curso. ¿Salir de todos modos?\n\nEl archivo que se está copiando se termina; la próxima ejecución hará el resto.",
            "Salir",
            "Seguir",
            "Diskman está sincronizando una copia.",
        ),
        "it" => (
            "È in corso un backup. Uscire comunque?\n\nIl file in copia viene completato; il resto lo farà la prossima esecuzione.",
            "Esci",
            "Continua",
            "Diskman sta sincronizzando un backup.",
        ),
        "nl" => (
            "Er loopt een back-up. Toch afsluiten?\n\nHet bestand dat nu wordt gekopieerd, wordt afgemaakt; de volgende keer doet de rest.",
            "Afsluiten",
            "Laten doorlopen",
            "Diskman synchroniseert een back-up.",
        ),
        "pt" => (
            "Está a decorrer um backup. Sair mesmo assim?\n\nO ficheiro que está a ser copiado é concluído; a próxima execução faz o resto.",
            "Sair",
            "Continuar",
            "O Diskman está a sincronizar um backup.",
        ),
        _ => (
            "A backup is running. Quit anyway?\n\nThe file being copied is finished first; the next run does the rest.",
            "Quit",
            "Keep running",
            "Diskman is syncing a backup.",
        ),
    };
    Texts { quit_question, quit, keep, busy }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::queue;

    #[test]
    fn stopping_empties_the_queue_and_cancels_the_running_sync() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from(dir.path().to_path_buf()).unwrap();
        queue::enqueue(&state, "a", "manual");
        assert!(state.try_begin_job("manual"));
        // The running sync does not end within the limit
        assert!(!stop_all(&state, Duration::from_millis(100)));
        assert!(state.cancel.load(Ordering::SeqCst));
        assert!(state.queue.lock().unwrap().is_empty());
        // Nothing new starts afterwards (see queue::take_next)
        assert!(state.quitting.load(Ordering::SeqCst));
        state.end_job();
        assert!(stop_all(&state, Duration::ZERO));
    }
}
