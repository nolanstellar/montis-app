//! MONTIS — LA COQUE NATIVE.
//!
//! Le cœur (le modèle, la mémoire, les règles) tourne sur le PC hébergeur ; cette application vit dans la barre des menus
//! du poste où elle est installée, charge l'interface Montis depuis le tunnel, et prête au cœur les mains que le navigateur
//! n'a pas : fichiers, applications, capture d'écran, presse-papiers, volume, impression, verrouillage, fenêtres.
//! Chaque commande est une fonction Rust, appelée par l'interface via `invoke`, exécutée SUR CETTE MACHINE.
//! Ce qui touche à l'utilisateur (mémoire, profil, sources) reste côté cœur : la coque ne stocke que l'adresse du cœur
//! et le raccourci.

mod poste;
mod pont;
mod autorisations;

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Réglages de la coque (l'adresse du cœur et le raccourci), dans le dossier de configuration de l'utilisateur.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Reglages {
    pub adresse_coeur: String,
    pub raccourci: String,
    pub compacte: bool,
    /// Identifiant stable de cet appareil auprès du cœur (créé une fois).
    #[serde(default)]
    pub appareil: String,
    /// L'écran des autorisations a été passé une fois.
    #[serde(default)]
    pub autorisations_faites: bool,
    /// Jeton de la porte du cœur (cookie montis_cle), gardé pour que le pont natif se connecte sans attendre la page.
    #[serde(default)]
    pub jeton: String,
}
impl Default for Reglages {
    fn default() -> Self {
        Self { adresse_coeur: "https://montis.agency-stellar.fr".into(), raccourci: if cfg!(target_os = "macos") { "Alt+Space".into() } else { "Ctrl+Space".into() }, compacte: true, appareil: String::new(), autorisations_faites: false, jeton: String::new() }
    }
}
fn identifiant_neuf() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let s = format!("{:x}{:x}", n, std::process::id());
    s.chars().rev().take(16).collect::<String>()
}
pub struct Etat(pub Mutex<Reglages>);

/// JOURNAL DE LA COQUE : chaque étape (démarrage, fenêtre, raccourci, clics, erreurs) dans <config>/journal.log, consultable
/// depuis le menu de l'icône. Une coque sans console doit pouvoir dire ce qu'elle a fait.
pub fn journaliser(app: &AppHandle, message: &str) {
    let d = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&d);
    let f = d.join("journal.log");
    if let Ok(m) = std::fs::metadata(&f) { if m.len() > 512 * 1024 { let _ = std::fs::rename(&f, d.join("journal.ancien.log")); } }
    let ligne = format!("{} {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), message);
    use std::io::Write;
    if let Ok(mut fic) = std::fs::OpenOptions::new().create(true).append(true).open(&f) { let _ = fic.write_all(ligne.as_bytes()); }
    eprintln!("[montis] {message}");
}
fn fichier_journal(app: &AppHandle) -> std::path::PathBuf { app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir()).join("journal.log") }

fn fichier_reglages(app: &AppHandle) -> std::path::PathBuf {
    let d = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&d);
    d.join("reglages.json")
}
fn lire_reglages(app: &AppHandle) -> Reglages {
    let mut r: Reglages = std::fs::read_to_string(fichier_reglages(app)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if r.appareil.is_empty() { r.appareil = identifiant_neuf(); ecrire_reglages(app, &r); }
    r
}
fn ecrire_reglages(app: &AppHandle, r: &Reglages) {
    let _ = std::fs::write(fichier_reglages(app), serde_json::to_string_pretty(r).unwrap_or_default());
}

#[tauri::command]
fn reglages(app: AppHandle) -> Reglages { lire_reglages(&app) }

#[tauri::command]
fn enregistrer_reglages(app: AppHandle, etat: tauri::State<Etat>, r: Reglages) -> Result<(), String> {
    let ancien = lire_reglages(&app);
    if ancien.raccourci != r.raccourci {
        let _ = app.global_shortcut().unregister_all();
        poser_raccourci(&app, &r.raccourci)?;
    }
    ecrire_reglages(&app, &r);
    *etat.0.lock().map_err(|e| e.to_string())? = r.clone();
    if ancien.adresse_coeur != r.adresse_coeur {
        if let Some(w) = app.get_webview_window("main") { let _ = w.navigate(r.adresse_coeur.parse::<tauri::Url>().map_err(|e| e.to_string())?); }
        if let Ok(mut l) = app.state::<pont::EtatPont>().0.lock() { l.coeur = r.adresse_coeur.clone(); l.jeton.clear(); }
    }
    Ok(())
}

/// L'écran des autorisations est passé : on le retient et la fenêtre charge le cœur.
#[tauri::command]
fn terminer_autorisations(app: AppHandle) -> Result<(), String> {
    let mut r = lire_reglages(&app); r.autorisations_faites = true; ecrire_reglages(&app, &r);
    journaliser(&app, "autorisations : écran passé");
    if let Some(w) = app.get_webview_window("main") { let _ = w.navigate(r.adresse_coeur.parse::<tauri::Url>().map_err(|e| e.to_string())?); }
    Ok(())
}
/// Rouvre l'écran des autorisations (menu).
fn ouvrir_autorisations(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") { let _ = w.navigate("tauri://localhost/autorisations.html".parse().unwrap()); }
    montrer_fenetre(app, false);
}

/// L'identifiant de cet appareil : l'écran l'utilise pour que le cœur adresse ses actions au bon poste.
#[tauri::command]
fn identifiant(app: AppHandle) -> String { lire_reglages(&app).appareil }

/// La page (qui a passé la porte) confie à la coque le jeton de la porte : le pont natif peut alors joindre le cœur par le tunnel.
#[tauri::command]
fn poser_jeton(app: AppHandle, pont: tauri::State<pont::EtatPont>, jeton: String) -> Result<(), String> {
    let j = jeton.trim().to_string();
    { let mut l = pont.0.lock().map_err(|e| e.to_string())?; if l.jeton == j { return Ok(()); } l.jeton = j.clone(); }
    let mut r = lire_reglages(&app); r.jeton = j; ecrire_reglages(&app, &r);
    journaliser(&app, "jeton de la porte reçu de la page : le pont se reconnecte");
    Ok(())
}

/// Version de la coque, pour l'interface et le support.
#[tauri::command]
fn version_coque() -> String { env!("CARGO_PKG_VERSION").to_string() }

#[tauri::command]
fn plateforme() -> String { std::env::consts::OS.to_string() }

/// La fenêtre principale, montrée et mise au premier plan.
#[tauri::command]
fn montrer(app: AppHandle) { montrer_fenetre(&app, false) }
#[tauri::command]
fn masquer(app: AppHandle) { if let Some(w) = app.get_webview_window("main") { let _ = w.minimize(); } }

fn montrer_fenetre(app: &AppHandle, compacte: bool) {
    let Some(w) = app.get_webview_window("main") else { journaliser(app, "montrer : fenêtre « main » introuvable"); return; };
    if compacte {
        if let Err(e) = w.set_size(tauri::LogicalSize::new(440.0, 640.0)) { journaliser(app, &format!("montrer : taille refusée : {e}")); }
        // Près du curseur, sans sortir de l'écran ; si l'écran est inconnu, on centre.
        match (w.cursor_position(), w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten())) {
            (Ok(pos), Some(mon)) => {
                let taille = mon.size(); let ech = mon.scale_factor(); let (mx, my) = (mon.position().x as f64, mon.position().y as f64);
                let x = (pos.x - 220.0 * ech).max(mx).min(mx + taille.width as f64 - 460.0 * ech);
                let y = (pos.y + 20.0 * ech).max(my).min(my + taille.height as f64 - 680.0 * ech).max(my);
                if let Err(e) = w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32)) { journaliser(app, &format!("montrer : position refusée : {e}")); }
                journaliser(app, &format!("montrer compacte : curseur ({:.0},{:.0}) écran {}x{} @{ech} → position ({:.0},{:.0})", pos.x, pos.y, taille.width, taille.height, x, y));
            }
            _ => { let _ = w.center(); journaliser(app, "montrer compacte : écran ou curseur inconnu → centrée"); }
        }
        let _ = w.set_always_on_top(true);
    } else {
        let _ = w.set_size(tauri::LogicalSize::new(1180.0, 800.0));
        let _ = w.center();
        let _ = w.set_always_on_top(false);
    }
    if let Err(e) = w.show() { journaliser(app, &format!("montrer : show refusé : {e}")); }
    if let Err(e) = w.unminimize() { journaliser(app, &format!("montrer : unminimize : {e}")); }
    if let Err(e) = w.set_focus() { journaliser(app, &format!("montrer : focus refusé : {e}")); }
    // Application d'arrière-plan (sans Dock) : macOS ne la met pas devant sans activation explicite.
    #[cfg(target_os = "macos")] { let a = app.clone(); let w2 = w.clone(); let _ = app.run_on_main_thread(move || { let _ = a.set_activation_policy(tauri::ActivationPolicy::Regular); let _ = w2.set_focus(); }); }
    let _ = w.emit("montis://appel", serde_json::json!({ "compacte": compacte }));
    journaliser(app, &format!("montrer {} : visible={:?} taille={:?} position={:?}", if compacte { "compacte" } else { "complète" }, w.is_visible(), w.outer_size().map(|s| (s.width, s.height)), w.outer_position().map(|p| (p.x, p.y))));
}

fn poser_raccourci(app: &AppHandle, texte: &str) -> Result<(), String> {
    let raccourci: Shortcut = texte.parse().map_err(|e| format!("raccourci « {texte} » invalide : {e:?}"))?;
    let _ = raccourci;
    let app2 = app.clone();
    app.global_shortcut()
        .on_shortcut(texte, move |_app, _sc, ev| {
            if ev.state() == ShortcutState::Pressed {
                let compacte = lire_reglages(&app2).compacte;
                journaliser(&app2, "raccourci");
                montrer_fenetre(&app2, compacte);
            }
        })
        .map_err(|e| e.to_string())
}

/// Cherche, télécharge et installe une mise à jour publiée ; redémarre l'application. Tout est journalisé.
async fn verifier_mise_a_jour(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let u = match app.updater() { Ok(u) => u, Err(e) => { journaliser(app, &format!("mise à jour : updater indisponible : {e}")); return; } };
    match u.check().await {
        Ok(Some(maj)) => {
            journaliser(app, &format!("mise à jour disponible : {} → {}", env!("CARGO_PKG_VERSION"), maj.version));
            let _ = app.emit("montis://maj", serde_json::json!({ "version": maj.version }));
            match maj.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => { journaliser(app, "mise à jour installée → redémarrage"); app.restart(); }
                Err(e) => journaliser(app, &format!("mise à jour : échec : {e}")),
            }
        }
        Ok(None) => journaliser(app, &format!("mise à jour : v{} est la dernière", env!("CARGO_PKG_VERSION"))),
        Err(e) => journaliser(app, &format!("mise à jour : vérification impossible : {e}")),
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| { journaliser(app, "seconde instance → on montre la fenêtre"); montrer_fenetre(app, false); }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Etat(Mutex::new(Reglages::default())))
        .manage(pont::EtatPont(std::sync::Arc::new(Mutex::new(pont::Liaison::default()))))
        .invoke_handler(tauri::generate_handler![
            reglages, enregistrer_reglages, version_coque, plateforme, montrer, masquer, identifiant, poser_jeton, terminer_autorisations,
            autorisations::etat_autorisations, autorisations::demander_autorisation, autorisations::ouvrir_reglage,
            poste::ouvrir_cible, poste::capture_ecran, poste::presse_papiers_lire, poste::presse_papiers_ecrire,
            poste::regler_volume, poste::regler_luminosite, poste::verrouiller, poste::mettre_en_veille, poste::imprimer,
            poste::infos_systeme, poste::chercher_fichiers, poste::lire_fichier, poste::creer_fichier, poste::renommer_fichier,
            poste::deplacer_fichier, poste::lister_dossier, poste::application, poste::fenetre, poste::notifier, poste::envoyer_message
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let premier_lancement = !fichier_reglages(&handle).exists();
            let r = lire_reglages(&handle);
            journaliser(&handle, &format!("démarrage v{} · {} · cœur {} · raccourci {} · appareil {}{}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, r.adresse_coeur, r.raccourci, r.appareil, if premier_lancement { " · PREMIER LANCEMENT" } else { "" }));
            *app.state::<Etat>().0.lock().unwrap() = r.clone();
            // Le pont natif : abonné au flux du cœur, il exécute les actions même fenêtre cachée.
            { let p = app.state::<pont::EtatPont>().0.clone(); if let Ok(mut l) = p.lock() { l.coeur = r.adresse_coeur.clone(); l.appareil = r.appareil.clone(); l.jeton = r.jeton.clone(); } pont::demarrer(handle.clone(), p); }
            // La fenêtre principale charge l'interface du cœur (mise à jour sans republier l'application).
            let url_coeur: tauri::Url = r.adresse_coeur.parse().unwrap_or_else(|_| "https://montis.agency-stellar.fr".parse().unwrap());
            let depart = if r.autorisations_faites { WebviewUrl::External(url_coeur.clone()) } else { WebviewUrl::App("autorisations.html".into()) };
            let url = url_coeur.clone();
            match WebviewWindowBuilder::new(app, "main", depart).title("Montis").inner_size(1180.0, 800.0).min_inner_size(380.0, 520.0).center().visible(true)
                .on_navigation({ let h = handle.clone(); move |u| { journaliser(&h, &format!("navigation → {u}")); true } })
                .on_page_load({ let h = handle.clone(); move |_w, p| { journaliser(&h, &format!("page {:?} : {}", p.event(), p.url())); } })
                .build() {
                Ok(_) => journaliser(&handle, &format!("fenêtre créée sur {url}")),
                Err(e) => { journaliser(&handle, &format!("ERREUR création de la fenêtre : {e}")); return Err(e.into()); }
            }
            // Fermer la fenêtre = fermer Montis (règle : l'interface est toujours visible tant que l'application tourne).
            if let Some(w) = app.get_webview_window("main") {
                let h3 = handle.clone();
                w.on_window_event(move |e| { if let tauri::WindowEvent::CloseRequested { .. } = e { journaliser(&h3, "fenêtre fermée → Montis se ferme"); h3.exit(0); } });
            }
            // MISE À JOUR AUTOMATIQUE : au démarrage puis toutes les six heures ; téléchargée, installée, redémarrage.
            { let h4 = handle.clone(); tauri::async_runtime::spawn(async move { loop {
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                verifier_mise_a_jour(&h4).await;
                tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
            } }); }
            // Barre des menus / zone de notification.
            let ouvrir = MenuItem::with_id(app, "ouvrir", "Ouvrir Montis", true, None::<&str>)?;
            let compacte = MenuItem::with_id(app, "compacte", "Appeler (fenêtre compacte)", true, None::<&str>)?;
            let reglages_item = MenuItem::with_id(app, "reglages", "Réglages de l'application…", true, None::<&str>)?;
            let journal_item = MenuItem::with_id(app, "journal", "Journal de la coque…", true, None::<&str>)?;
            let autorisations_item = MenuItem::with_id(app, "autorisations", "Autorisations…", true, None::<&str>)?;
            let maj = MenuItem::with_id(app, "maj", "Rechercher une mise à jour", true, None::<&str>)?;
            let quitter = MenuItem::with_id(app, "quitter", "Quitter Montis", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&ouvrir, &compacte, &PredefinedMenuItem::separator(app)?, &reglages_item, &autorisations_item, &journal_item, &maj, &PredefinedMenuItem::separator(app)?, &quitter])?;
            let h = handle.clone();
            TrayIconBuilder::with_id("montis")
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true)
                .tooltip("Montis")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, ev| match ev.id().as_ref() {
                    "autorisations" => { journaliser(app, "menu : autorisations"); ouvrir_autorisations(app) }
                    "journal" => { let f = fichier_journal(app); journaliser(app, "journal ouvert"); let _ = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "notepad" }).arg(&f).spawn(); }
                    "ouvrir" => { journaliser(app, "menu : ouvrir"); montrer_fenetre(app, false) }
                    "compacte" => montrer_fenetre(app, true),
                    "reglages" => { montrer_fenetre(app, false); let _ = app.emit("montis://reglages", ()); }
                    "maj" => { let h5 = app.clone(); tauri::async_runtime::spawn(async move { verifier_mise_a_jour(&h5).await; }); montrer_fenetre(app, false); }
                    "quitter" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(move |_tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        journaliser(&h, "clic sur l'icône");
                        let compacte = lire_reglages(&h).compacte;
                        montrer_fenetre(&h, compacte);
                    }
                })
                .build(app)?;
            journaliser(&handle, "icône posée dans la barre (Dock et barre des menus)");
            // Raccourci global.
            match poser_raccourci(&handle, &r.raccourci) { Ok(()) => journaliser(&handle, &format!("raccourci {} enregistré", r.raccourci)), Err(e) => journaliser(&handle, &format!("ERREUR raccourci : {e}")) }
            // Premier lancement : la fenêtre s'affiche d'elle-même, complète, pour l'accueil.
            if premier_lancement { let h2 = handle.clone(); std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(800)); montrer_fenetre(&h2, false); }); }
            // Démarrage automatique avec la session.
            use tauri_plugin_autostart::ManagerExt;
            let _ = app.autolaunch().enable();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Montis n'a pas pu démarrer");
}
