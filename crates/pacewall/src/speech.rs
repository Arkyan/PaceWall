//! Voix des alertes. Deux moteurs :
//! - **ingénieur de course** : des clips enregistrés à l'avance (voir `tools/voice/generate.py`), avec effet radio et
//!   plusieurs variantes par phrase. Intégrés au programme, lus par la carte son : rien à calculer pendant la course ;
//! - **voix Windows** (SAPI), hors ligne : secours pour les phrases sans clip ou si on le choisit dans les réglages.
//!
//! La parole se fait sur un fil à part : le dessin de l'overlay n'attend jamais la fin d'une phrase.

use crate::voice_clips::{CLIPS_F, CLIPS_M};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
use windows::Win32::Media::Speech::{ISpObjectToken, ISpObjectTokenCategory, ISpVoice, SpObjectTokenCategory, SpVoice, SPCAT_VOICES};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

/// Qui parle : l'ingénieur (voix d'homme ou de femme, clips enregistrés) ou la voix de synthèse de Windows.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Engine {
    Male,
    Female,
    Windows,
}

impl Engine {
    /// Valeur du réglage « voice_engine » (l'ancienne valeur « engineer » donne la voix d'homme).
    pub fn from_setting(value: &str) -> Self {
        match value {
            "windows" => Engine::Windows,
            "engineer_f" => Engine::Female,
            _ => Engine::Male,
        }
    }

    fn clips(self) -> Option<&'static [(&'static str, &'static [&'static [u8]])]> {
        match self {
            Engine::Male => Some(CLIPS_M),
            Engine::Female => Some(CLIPS_F),
            Engine::Windows => None,
        }
    }
}

struct Say {
    /// Clé du clip de l'ingénieur (« fuel_3 », « yellow »…), ou le texte lui-même pour un essai.
    key: String,
    text: String,
    volume: u16,
    engine: Engine,
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

    /// Dit une alerte à `volume` % (0 à 100). `key` désigne le clip de l'ingénieur ; `text` est lu par la voix Windows
    /// si `engine` est la voix Windows ou si la clé n'a pas de clip. Ne bloque pas : la phrase est lue par le fil de parole.
    pub fn say(&self, key: &str, text: &str, volume: u32, engine: Engine) {
        let _ = self.tx.send(Say { key: key.to_string(), text: text.to_string(), volume: volume.min(100) as u16, engine });
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

/// Copie d'un clip WAV (PCM 16 bits) dont le niveau est ramené à `volume` % : la lecture système n'a pas de réglage propre.
fn with_volume(wav: &[u8], volume: u16) -> Vec<u8> {
    let mut out = wav.to_vec();
    // Début des échantillons : juste après le bloc « data » et sa taille.
    let Some(pos) = out.windows(4).position(|w| w == b"data") else { return out };
    let gain = (volume as f32 / 100.0).powf(1.6); // courbe douce : 50 % semble moitié moins fort à l'oreille
    for s in out[pos + 8..].chunks_exact_mut(2) {
        let v = (i16::from_le_bytes([s[0], s[1]]) as f32 * gain).round() as i16;
        s.copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// Joue un clip jusqu'au bout (bloquant, le fil de parole est dédié à ça).
fn play_clip(wav: &[u8], volume: u16) {
    let data = with_volume(wav, volume);
    unsafe {
        if !PlaySoundW(PCWSTR(data.as_ptr() as *const u16), None, SND_MEMORY | SND_SYNC | SND_NODEFAULT).as_bool() {
            note("lecture du clip impossible (aucune sortie audio ?)");
        }
    }
}

fn worker(rx: Receiver<Say>) {
    let _ = std::fs::remove_file(std::env::temp_dir().join("pw-speech.log"));
    // Variante jouée en dernier pour chaque clé : on alterne pour ne jamais redire la même phrase deux fois de suite.
    let mut last_variant: HashMap<(Engine, String), usize> = HashMap::new();
    let mut counter = 0usize;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        // La voix Windows n'est préparée qu'au premier besoin.
        let mut sapi: Option<Option<ISpVoice>> = None;
        while let Ok(first) = rx.recv() {
            // Si plusieurs phrases se sont accumulées, on ne garde que les deux dernières : une alerte lue en retard
            // ne sert plus à rien.
            let mut pending = vec![first];
            while let Ok(next) = rx.try_recv() {
                pending.push(next);
            }
            let skip = pending.len().saturating_sub(2);
            for say in pending.into_iter().skip(skip) {
                if let Some(table) = say.engine.clips() {
                    if let Some((_, variants)) = table.iter().find(|(k, _)| *k == say.key) {
                        counter += 1;
                        let previous = last_variant.get(&(say.engine, say.key.clone())).copied();
                        // Variante au hasard (compteur mélangé), différente de la précédente.
                        let mut pick = (counter.wrapping_mul(2654435761) >> 7) % variants.len();
                        if variants.len() > 1 && Some(pick) == previous {
                            pick = (pick + 1) % variants.len();
                        }
                        last_variant.insert((say.engine, say.key.clone()), pick);
                        play_clip(variants[pick], say.volume);
                        continue;
                    }
                }
                let voice = sapi.get_or_insert_with(|| open_sapi()).as_ref();
                let Some(v) = voice else { continue };
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

/// Prépare la voix Windows (voix française si possible).
fn open_sapi() -> Option<ISpVoice> {
    unsafe {
        let voice: ISpVoice = match CoCreateInstance(&SpVoice, None, CLSCTX_ALL) {
            Ok(v) => v,
            Err(e) => {
                note(&format!("voix Windows indisponible : {e}"));
                return None;
            }
        };
        match french_voice() {
            Some(token) => {
                if let Err(e) = voice.SetVoice(&token) {
                    note(&format!("voix française refusée : {e}"));
                }
            }
            None => note("aucune voix française installée : voix par défaut de Windows"),
        }
        Some(voice)
    }
}

/// Journal des problèmes de voix : %TEMP%\pw-speech.log (écrasé à chaque lancement du fil de parole).
fn note(text: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("pw-speech.log")) {
        let _ = writeln!(f, "{text}");
    }
}
