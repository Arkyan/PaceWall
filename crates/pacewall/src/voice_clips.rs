//! Généré par tools/voice/generate.py : ne pas modifier à la main.

/// Clips de la voix d'ingénieur : (clé d'alerte, variantes WAV 16 bits mono 16 kHz).
pub const CLIPS: &[(&str, &[&[u8]])] = &[
    ("fuel_5", &[include_bytes!("../assets/voice/fuel_5_0.wav"), include_bytes!("../assets/voice/fuel_5_1.wav")]),
    ("fuel_3", &[include_bytes!("../assets/voice/fuel_3_0.wav"), include_bytes!("../assets/voice/fuel_3_1.wav")]),
    ("fuel_2", &[include_bytes!("../assets/voice/fuel_2_0.wav"), include_bytes!("../assets/voice/fuel_2_1.wav")]),
    ("fuel_1", &[include_bytes!("../assets/voice/fuel_1_0.wav"), include_bytes!("../assets/voice/fuel_1_1.wav")]),
    ("energy_3", &[include_bytes!("../assets/voice/energy_3_0.wav"), include_bytes!("../assets/voice/energy_3_1.wav")]),
    ("energy_2", &[include_bytes!("../assets/voice/energy_2_0.wav"), include_bytes!("../assets/voice/energy_2_1.wav")]),
    ("energy_1", &[include_bytes!("../assets/voice/energy_1_0.wav"), include_bytes!("../assets/voice/energy_1_1.wav")]),
    ("tyres_cold", &[include_bytes!("../assets/voice/tyres_cold_0.wav"), include_bytes!("../assets/voice/tyres_cold_1.wav")]),
    ("tyres_hot", &[include_bytes!("../assets/voice/tyres_hot_0.wav"), include_bytes!("../assets/voice/tyres_hot_1.wav")]),
    ("tyres_worn", &[include_bytes!("../assets/voice/tyres_worn_0.wav"), include_bytes!("../assets/voice/tyres_worn_1.wav")]),
    ("flat", &[include_bytes!("../assets/voice/flat_0.wav"), include_bytes!("../assets/voice/flat_1.wav")]),
    ("yellow", &[include_bytes!("../assets/voice/yellow_0.wav"), include_bytes!("../assets/voice/yellow_1.wav")]),
    ("fcy", &[include_bytes!("../assets/voice/fcy_0.wav"), include_bytes!("../assets/voice/fcy_1.wav")]),
    ("green", &[include_bytes!("../assets/voice/green_0.wav"), include_bytes!("../assets/voice/green_1.wav")]),
    ("rain", &[include_bytes!("../assets/voice/rain_0.wav"), include_bytes!("../assets/voice/rain_1.wav")]),
    ("test", &[include_bytes!("../assets/voice/test_0.wav"), include_bytes!("../assets/voice/test_1.wav")]),
];
