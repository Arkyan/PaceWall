//! Synthèse vocale : la voix intégrée à Windows (SAPI), hors ligne, sans rien embarquer ni télécharger.
//! Une voix française est choisie d'office si elle est installée (Paramètres > Heure et langue > Voix).
//! La parole se fait sur un fil à part : le dessin de l'overlay n'attend jamais la fin d'une phrase.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Media::Speech::{ISpObjectToken, ISpObjectTokenCategory, ISpVoice, SpObjectTokenCategory, SpVoice, SPCAT_VOICES};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

struct Say {
    text: String,
    volume: u16,
}

pub struct Speaker {
    tx: Sender<Say>,
}

/// SPF_IS_NOT_XML : le texte est lu tel quel, sans interpréter de balises.
const SPF_IS_NOT_XML: u32 = 16;

impl Speaker {
    fn new() -> Self {
        let (tx, rx) = channel();
        std::thread::Builder::new().name("pw-speech".into()).spawn(move || worker(rx)).ok();
        Self { tx }
    }

    /// Le haut-parleur du processus (créé au premier usage).
    pub fn global() -> &'static Speaker {
        static SPEAKER: OnceLock<Speaker> = OnceLock::new();
        SPEAKER.get_or_init(Speaker::new)
    }

    /// Dit `text` à `volume` % (0 à 100). Ne bloque pas : la phrase est lue par le fil de parole.
    pub fn say(&self, text: &str, volume: u32) {
        let _ = self.tx.send(Say { text: text.to_string(), volume: volume.min(100) as u16 });
    }
}

/// Voix française si Windows en a une, sinon la voix par défaut.
unsafe fn french_voice() -> Option<ISpObjectToken> {
    let category: ISpObjectTokenCategory = CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL).ok()?;
    category.SetId(SPCAT_VOICES, false).ok()?;
    // Langue 40C = français (France) ; 80C = français (Belgique), 100C = français (Suisse) : la première suffit.
    for lang in [w!("Language=40C"), w!("Language=80C"), w!("Language=100C")] {
        if let Ok(tokens) = category.EnumTokens(lang, PCWSTR::null()) {
            if let Ok(token) = tokens.Item(0) {
                return Some(token);
            }
        }
    }
    None
}

fn worker(rx: Receiver<Say>) {
    let _ = std::fs::remove_file(std::env::temp_dir().join("pw-speech.log"));
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let voice: Option<ISpVoice> = match CoCreateInstance(&SpVoice, None, CLSCTX_ALL) {
            Ok(v) => Some(v),
            Err(e) => {
                note(&format!("voix indisponible : {e}"));
                None
            }
        };
        if let Some(v) = &voice {
            match french_voice() {
                Some(token) => {
                    if let Err(e) = v.SetVoice(&token) {
                        note(&format!("voix française refusée : {e}"));
                    }
                }
                None => note("aucune voix française installée : voix par défaut de Windows"),
            }
        }
        while let Ok(first) = rx.recv() {
            // Si plusieurs phrases se sont accumulées, on ne garde que les deux dernières : une alerte lue en retard
            // ne sert plus à rien.
            let mut pending = vec![first];
            while let Ok(next) = rx.try_recv() {
                pending.push(next);
            }
            let skip = pending.len().saturating_sub(2);
            let Some(v) = &voice else { continue };
            for say in pending.into_iter().skip(skip) {
                let _ = v.SetVolume(say.volume);
                let text: Vec<u16> = say.text.encode_utf16().chain(Some(0)).collect();
                // Lecture bloquante : les phrases se suivent sans se couper.
                if let Err(e) = v.Speak(PCWSTR(text.as_ptr()), SPF_IS_NOT_XML, None) {
                    note(&format!("lecture impossible : {e}"));
                }
            }
        }
    }
}

/// Journal des problèmes de voix : %TEMP%\pw-speech.log (écrasé à chaque lancement du fil de parole).
fn note(text: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("pw-speech.log")) {
        let _ = writeln!(f, "{text}");
    }
}
