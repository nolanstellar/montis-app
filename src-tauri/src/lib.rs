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
}
impl Default for Reglages {
    fn default() -> Self {
        Self { adresse_coeur: "https://montis.agency-stellar.fr".into(), raccourci: if cfg!(target_os = "macos") { "Alt+Space".into() } else { "Ctrl+Space".into() }, compacte: true, appareil: String::new() }
    }
}
fn identifiant_neuf() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let s = format!("{:x}{:x}", n, std::process::id());
    s.chars().rev().take(16).collect::<String>()
}
pub struct Etat(pub Mutex<Reglages>);

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

/// L'identifiant de cet appareil : l'écran l'utilise pour que le cœur adresse ses actions au bon poste.
#[tauri::command]
fn identifiant(app: AppHandle) -> String { lire_reglages(&app).appareil }

/// La page (qui a passé la porte) confie à la coque le jeton de la porte : le pont natif peut alors joindre le cœur par le tunnel.
#[tauri::command]
fn poser_jeton(pont: tauri::State<pont::EtatPont>, jeton: String) -> Result<(), String> {
    let mut l = pont.0.lock().map_err(|e| e.to_string())?;
    l.jeton = jeton.trim().to_string();
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
fn masquer(app: AppHandle) { if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); } }

fn montrer_fenetre(app: &AppHandle, compacte: bool) {
    if let Some(w) = app.get_webview_window("main") {
        if compacte {
            let _ = w.set_size(tauri::LogicalSize::new(440.0, 640.0));
            // Près du curseur, sans sortir de l'écran.
            if let (Ok(pos), Ok(Some(mon))) = (w.cursor_position(), w.current_monitor()) {
                let taille = mon.size(); let ech = mon.scale_factor();
                let x = (pos.x - 220.0 * ech).max(mon.position().x as f64).min((mon.position().x + taille.width as i32) as f64 - 460.0 * ech);
                let y = (pos.y + 20.0 * ech).min((mon.position().y + taille.height as i32) as f64 - 680.0 * ech);
                let _ = w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
            }
            let _ = w.set_always_on_top(true);
        } else {
            let _ = w.set_size(tauri::LogicalSize::new(1180.0, 800.0));
            let _ = w.center();
            let _ = w.set_always_on_top(false);
        }
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("montis://appel", serde_json::json!({ "compacte": compacte }));
    }
}

fn poser_raccourci(app: &AppHandle, texte: &str) -> Result<(), String> {
    let raccourci: Shortcut = texte.parse().map_err(|e| format!("raccourci « {texte} » invalide : {e:?}"))?;
    let _ = raccourci;
    let app2 = app.clone();
    app.global_shortcut()
        .on_shortcut(texte, move |_app, _sc, ev| {
            if ev.state() == ShortcutState::Pressed {
                let compacte = lire_reglages(&app2).compacte;
                match app2.get_webview_window("main") {
                    Some(w) if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) => { let _ = w.hide(); }
                    _ => montrer_fenetre(&app2, compacte),
                }
            }
        })
        .map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| { montrer_fenetre(app, false); }))
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
            reglages, enregistrer_reglages, version_coque, plateforme, montrer, masquer, identifiant, poser_jeton,
            poste::ouvrir_cible, poste::capture_ecran, poste::presse_papiers_lire, poste::presse_papiers_ecrire,
            poste::regler_volume, poste::regler_luminosite, poste::verrouiller, poste::mettre_en_veille, poste::imprimer,
            poste::infos_systeme, poste::chercher_fichiers, poste::lire_fichier, poste::creer_fichier, poste::renommer_fichier,
            poste::deplacer_fichier, poste::lister_dossier, poste::application, poste::fenetre, poste::notifier
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let r = lire_reglages(&handle);
            *app.state::<Etat>().0.lock().unwrap() = r.clone();
            // Le pont natif : abonné au flux du cœur, il exécute les actions même fenêtre cachée.
            { let p = app.state::<pont::EtatPont>().0.clone(); if let Ok(mut l) = p.lock() { l.coeur = r.adresse_coeur.clone(); l.appareil = r.appareil.clone(); } pont::demarrer(handle.clone(), p); }
            // La fenêtre principale charge l'interface du cœur (mise à jour sans republier l'application).
            let url: tauri::Url = r.adresse_coeur.parse().unwrap_or_else(|_| "https://montis.agency-stellar.fr".parse().unwrap());
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url)).title("Montis").inner_size(1180.0, 800.0).min_inner_size(380.0, 520.0).center().visible(false).build()?;
            // La fenêtre se cache au lieu de se fermer : Montis reste dans la barre.
            if let Some(w) = app.get_webview_window("main") {
                let w2 = w.clone();
                w.on_window_event(move |e| { if let tauri::WindowEvent::CloseRequested { api, .. } = e { api.prevent_close(); let _ = w2.hide(); } });
            }
            // Barre des menus / zone de notification.
            let ouvrir = MenuItem::with_id(app, "ouvrir", "Ouvrir Montis", true, None::<&str>)?;
            let compacte = MenuItem::with_id(app, "compacte", "Appeler (fenêtre compacte)", true, None::<&str>)?;
            let reglages_item = MenuItem::with_id(app, "reglages", "Réglages de l'application…", true, None::<&str>)?;
            let maj = MenuItem::with_id(app, "maj", "Rechercher une mise à jour", true, None::<&str>)?;
            let quitter = MenuItem::with_id(app, "quitter", "Quitter Montis", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&ouvrir, &compacte, &PredefinedMenuItem::separator(app)?, &reglages_item, &maj, &PredefinedMenuItem::separator(app)?, &quitter])?;
            let h = handle.clone();
            TrayIconBuilder::with_id("montis")
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true)
                .tooltip("Montis")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, ev| match ev.id().as_ref() {
                    "ouvrir" => montrer_fenetre(app, false),
                    "compacte" => montrer_fenetre(app, true),
                    "reglages" => { montrer_fenetre(app, false); let _ = app.emit("montis://reglages", ()); }
                    "maj" => { let _ = app.emit("montis://verifier-maj", ()); montrer_fenetre(app, false); }
                    "quitter" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(move |_tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        let compacte = lire_reglages(&h).compacte;
                        montrer_fenetre(&h, compacte);
                    }
                })
                .build(app)?;
            // Raccourci global.
            if let Err(e) = poser_raccourci(&handle, &r.raccourci) { eprintln!("[montis] raccourci : {e}"); }
            // Démarrage automatique avec la session.
            use tauri_plugin_autostart::ManagerExt;
            let _ = app.autolaunch().enable();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Montis n'a pas pu démarrer");
}
