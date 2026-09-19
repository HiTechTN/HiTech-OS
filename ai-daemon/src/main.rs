//! HiTech-OS AI Daemon — entry point.
//!
//! Phase 4 : integration reelle du backend GGUF/llama.cpp avec chargement
//! mmap zero-copy (comportement par defaut de llama.cpp), + backend CUDA
//! optionnel derriere le trait `InferenceBackend` pour les deploiements
//! serveur hybride (feature `cuda`).
//!
//! Le chemin du modele GGUF est lu depuis la variable d'environnement
//! `HITECHOS_MODEL_PATH`. Si elle est absente, le demon demarre quand
//! meme (utile pour valider le reste de l'OS sans avoir de modele sous
//! la main) mais ne charge rien.

mod backend;

use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("hitechos-ai-daemon — démarrage");

    let mut backend = backend::select_backend()?;
    println!("Backend d'inférence sélectionné : {}", backend.name());

    match env::var("HITECHOS_MODEL_PATH") {
        Ok(path) => {
            let path = PathBuf::from(path);
            println!("Chargement du modèle GGUF : {path:?}");
            backend.load_model(&path)?;

            let prompt = "Bonjour, je suis";
            let output = backend.generate(prompt, Some(64))?;
            println!("Prompt : {prompt}");
            println!("Sortie : {output}");
        }
        Err(_) => {
            println!(
                "HITECHOS_MODEL_PATH non définie — démon prêt, aucun modèle chargé \
                 (fixe cette variable vers un fichier .gguf pour tester l'inférence)."
            );
        }
    }

    // TODO Phase 5 (AgentOS) :
    // - exposer une interface (socket local / MQTT) pour que l'agent
    //   consomme ce backend au lieu d'un appel direct en ligne de commande
    Ok(())
}
