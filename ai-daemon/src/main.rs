//! HiTech-OS AI Daemon — entry point.
//!
//! Phase 4 (roadmap) : intégration réelle du backend GGUF/llama.cpp
//! avec chargement mmap zero-copy, + backend CUDA optionnel derrière
//! le trait `InferenceBackend` pour les déploiements serveur hybride.

mod backend;

#[tokio::main]
async fn main() {
    println!("hitechos-ai-daemon — démarrage (squelette Phase 4)");

    let backend = backend::select_backend();
    println!("Backend d'inférence sélectionné : {}", backend.name());

    // TODO Phase 4 :
    // - charger un modèle GGUF quantifié (4-bit/2-bit) via mmap
    // - exposer une interface (socket local / MQTT) pour AgentOS
}
