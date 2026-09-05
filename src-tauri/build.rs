fn main() {
    // Les commandes de l'application sont appelées depuis une page DISTANTE (le cœur) : elles doivent figurer dans la liste d'accès.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&["reglages", "enregistrer_reglages", "version_coque", "plateforme", "montrer", "masquer", "identifiant", "poser_jeton", "ouvrir_cible", "capture_ecran", "presse_papiers_lire", "presse_papiers_ecrire", "regler_volume", "regler_luminosite", "verrouiller", "mettre_en_veille", "imprimer", "infos_systeme", "chercher_fichiers", "lire_fichier", "creer_fichier", "renommer_fichier", "deplacer_fichier", "lister_dossier", "application", "fenetre", "notifier"])))
        .expect("construction Tauri");
}
