"""Fabrique les phrases de la voix d'ingénieur de course (alertes vocales de PaceWall).

Chaque phrase est synthétisée avec Piper (voix neuronale hors ligne), en plusieurs variantes, puis passée à l'effet
« radio » : bande passante réduite, compression douce, souffle léger et petits bips de début et de fin. Les clips sont
écrits dans crates/pacewall/assets/voice/<voix>/ et listés dans crates/pacewall/src/voice_clips.rs (intégrés au programme :
rien à télécharger ni à calculer pendant la course).

Voix :
  f (femme)  fr_FR-siwis-medium, données SIWIS, licence CC BY 4.0 (Université d'Édimbourg)
  m (homme)  fr_FR-upmc-medium, locuteur « pierre », données UPMC, licence CC BY-SA 4.0 (Université Pierre et Marie Curie)
Les modèles ne sont pas dans le dépôt (rhasspy/piper-voices sur HuggingFace).

Utilisation :
    pip install piper-tts numpy scipy
    python tools/voice/generate.py f chemin/vers/fr_FR-siwis-medium.onnx
    python tools/voice/generate.py m chemin/vers/fr_FR-upmc-medium.onnx 1      # 1 = locuteur « pierre »
"""
import sys
from pathlib import Path

import numpy as np
from piper import PiperVoice, SynthesisConfig
from scipy.io import wavfile
from scipy.signal import butter, resample_poly, sosfilt

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "crates" / "pacewall" / "assets" / "voice"
RS = ROOT / "crates" / "pacewall" / "src" / "voice_clips.rs"
SR = 16000  # la radio est limitée à ~3,4 kHz : 16 kHz suffit largement et garde les fichiers petits

# Voix calme / voix pressée (longueur plus courte, plus de variation) : l'ingénieur s'adapte à l'urgence.
CALM = [dict(length_scale=1.0, noise_scale=0.62, noise_w_scale=0.75), dict(length_scale=0.96, noise_scale=0.72, noise_w_scale=0.85)]
URGENT = [dict(length_scale=0.88, noise_scale=0.78, noise_w_scale=0.9), dict(length_scale=0.84, noise_scale=0.85, noise_w_scale=0.95)]

PHRASES = {
    "fuel_5": (CALM, ["Carburant, il reste cinq tours.", "Cinq tours de carburant."]),
    "fuel_3": (CALM, ["Attention carburant, trois tours restants.", "Trois tours de carburant, prépare ton arrêt."]),
    "fuel_2": (URGENT, ["Carburant critique, deux tours.", "Plus que deux tours de carburant, rentre au stand."]),
    "fuel_1": (URGENT, ["Dernier tour de carburant, rentre au stand maintenant !", "Un tour de carburant, rentre immédiatement !"]),
    "energy_3": (CALM, ["Énergie, trois tours restants.", "Attention à l'énergie, trois tours."]),
    "energy_2": (URGENT, ["Énergie critique, deux tours.", "Plus que deux tours d'énergie."]),
    "energy_1": (URGENT, ["Dernier tour d'énergie, rentre au stand !", "Un tour d'énergie, rentre maintenant !"]),
    "tyres_cold": (CALM, ["Pneus froids, fais-les monter en température.", "Attention, tes pneus sont froids."]),
    "tyres_hot": (CALM, ["Pneus trop chauds, ménage-les.", "Température des pneus trop élevée, lève un peu le pied."]),
    "tyres_worn": (CALM, ["Pneus usés, pense à ton arrêt.", "Les pneus sont en fin de vie."]),
    "flat": (URGENT, ["Crevaison ! Rentre au stand !", "Tu as une crevaison, rentre doucement !"]),
    "yellow": (URGENT, ["Drapeau jaune.", "Drapeau jaune, lève le pied."]),
    "fcy": (CALM, ["Full course jaune.", "Full course jaune, respecte les écarts et reste concentré."]),
    "green": (CALM, ["Course relancée, drapeau vert.", "Feu vert, c'est reparti, bonne relance."]),
    "rain": (CALM, ["Pluie détectée.", "Il commence à pleuvoir, prépare tes pneus pluie."]),
    "test": (CALM, ["Radio check. Ici ton ingénieur, tu m'entends bien ?", "Alertes vocales activées, je suis avec toi."]),
}


def band(sig, lo, hi):
    return sosfilt(butter(4, [lo, hi], btype="bandpass", fs=SR, output="sos"), sig)


def radio(voice_audio, sr_in, rng):
    """Effet radio d'équipe : bande étroite, compression, souffle, bips de début et de fin."""
    x = resample_poly(voice_audio, SR, sr_in).astype(np.float64)
    x = band(x, 320, 3300)
    x = np.tanh(2.4 * x) / np.tanh(2.4)  # compression douce : la voix reste « serrée », comme dans un casque
    x = band(x, 300, 3400)
    x /= max(np.abs(x).max(), 1e-6)

    def burst(ms, level, lo, hi):
        n = int(SR * ms / 1000)
        env = np.exp(-np.linspace(0, 6, n))
        return band(rng.standard_normal(n), lo, hi) * env * level

    lead, tail = int(SR * 0.16), int(SR * 0.22)
    out = np.zeros(lead + len(x) + tail)
    out[lead : lead + len(x)] += x * 0.82
    # souffle de la liaison radio pendant toute la phrase
    out += band(rng.standard_normal(len(out)), 1800, 3300) * 0.012
    # bips : ouverture du micro (début) et relâchement (fin)
    click_in = burst(70, 0.42, 900, 3200)
    out[: len(click_in)] += click_in
    click_out = burst(110, 0.34, 700, 3000)
    out[lead + len(x) + 10 : lead + len(x) + 10 + len(click_out)] += click_out
    out /= max(np.abs(out).max(), 1e-6)
    return (out * 0.89 * 32767).astype(np.int16)


def write_listing():
    """Liste tous les clips présents (toutes les voix) dans voice_clips.rs."""
    lines = ["//! Généré par tools/voice/generate.py : ne pas modifier à la main.", ""]
    for name, const in (("f", "CLIPS_F"), ("m", "CLIPS_M")):
        folder = ASSETS / name
        keys = {}
        for wav in sorted(folder.glob("*.wav")) if folder.exists() else []:
            keys.setdefault(wav.stem.rsplit("_", 1)[0], []).append(wav.name)
        lines.append(f"/// Clips de la voix « {name} » : (clé d'alerte, variantes WAV 16 bits mono 16 kHz).")
        lines.append(f"pub const {const}: &[(&str, &[&[u8]])] = &[")
        for key, names in keys.items():
            files = ", ".join(f'include_bytes!("../assets/voice/{name}/{n}")' for n in names)
            lines.append(f'    ("{key}", &[{files}]),')
        lines.append("];")
        lines.append("")
    RS.write_text("\n".join(lines), encoding="utf-8")


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ("f", "m"):
        sys.exit(__doc__)
    name, model = sys.argv[1], sys.argv[2]
    speaker = int(sys.argv[3]) if len(sys.argv) > 3 else None
    voice = PiperVoice.load(model)
    out_dir = ASSETS / name
    out_dir.mkdir(parents=True, exist_ok=True)
    for old in out_dir.glob("*.wav"):
        old.unlink()
    rng = np.random.default_rng(7)
    count = 0
    for key, (params, texts) in PHRASES.items():
        for i, text in enumerate(texts):
            chunks = list(voice.synthesize(text, syn_config=SynthesisConfig(speaker_id=speaker, **params[i % len(params)])))
            audio = np.concatenate([c.audio_float_array for c in chunks])
            clip = radio(audio, chunks[0].sample_rate, rng)
            wavfile.write(out_dir / f"{key}_{i}.wav", SR, clip)
            count += 1
            print(f"{key}_{i}.wav  {len(clip) / SR:4.1f} s")
    write_listing()
    total = sum(p.stat().st_size for p in out_dir.glob("*.wav"))
    print(f"\n{count} clips, {total / 1e6:.2f} Mo (voix {name})")


if __name__ == "__main__":
    main()
