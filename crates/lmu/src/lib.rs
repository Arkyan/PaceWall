//! Structures de la mémoire partagée `LMU_Data` de Le Mans Ultimate.
//!
//! Disposition reprise de `SharedMemoryInterface.hpp` et `InternalsPlugin.hpp` (fournis avec le jeu),
//! via le portage Python `pyLMUSharedMemory` (MIT, © 2021 Tony Whitley, © 2025 Xiang).
//! Tout est en `packed(4)`. Les booléens C++ sont lus en `u8` (pas de valeur invalide possible).
//! Les champs d'une structure `packed` se lisent par copie (jamais par référence).

pub mod data;
pub mod reader;

pub use data::*;
