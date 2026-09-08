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

/// Ouvre l'écran des autorisations système depuis la page (bouton « Autorisations système » des Réglages) : plus besoin du clic droit.
#[tauri::command]
fn autorisations_systeme(app: AppHandle) { journaliser(&app, "autorisations : ouvertes depuis les Réglages de l'écran"); ouvrir_autorisations(&app); }

/// Redémarre Montis (après avoir coché une autorisation que macOS n'applique qu'au relancement).
#[tauri::command]
fn relancer(app: AppHandle) { journaliser(&app, "relance demandée depuis l'écran des autorisations"); app.restart(); }

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
                // Le raccourci est aussi le RÉVEIL de secours (démo, jour 5) : la page ouvre l'écoute sans attendre le mot d'éveil.
                let _ = app2.emit("montis://ecoute", ());
            }
        })
        .map_err(|e| e.to_string())
}

/// Deux canaux : l'application STABLE (identifiant fr.agency-stellar.montis) suit les versions publiées et installe la mise à jour en
/// arrière-plan, active au PROCHAIN lancement, sans fenêtre ni redémarrage ; l'application BÊTA (identifiant …montis.beta) suit les
/// pré-versions et redémarre aussitôt — c'est l'application de test, installée à côté, qui ne touche jamais à la stable.
fn est_beta(app: &AppHandle) -> bool { app.config().identifier.ends_with(".beta") }

/// Cherche, télécharge et installe une mise à jour publiée. Tout est journalisé.
async fn verifier_mise_a_jour(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let u = match app.updater() { Ok(u) => u, Err(e) => { journaliser(app, &format!("mise à jour : updater indisponible : {e}")); return; } };
    match u.check().await {
        Ok(Some(maj)) => {
            journaliser(app, &format!("mise à jour disponible : {} → {}", env!("CARGO_PKG_VERSION"), maj.version));
            let _ = app.emit("montis://maj", serde_json::json!({ "version": maj.version }));
            match maj.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    if est_beta(app) { journaliser(app, "mise à jour installée → redémarrage (canal bêta)"); app.restart(); }
                    else { journaliser(app, &format!("mise à jour {} installée en arrière-plan : active au prochain lancement de Montis (canal stable)", maj.version)); }
                }
                Err(e) => journaliser(app, &format!("mise à jour : échec : {e}")),
            }
        }
        Ok(None) => journaliser(app, &format!("mise à jour : v{} est la dernière", env!("CARGO_PKG_VERSION"))),
        Err(e) => journaliser(app, &format!("mise à jour : vérification impossible : {e}")),
    }
}

/// UNE SEULE ETOILE DANS LA BARRE (08/09). Depuis la 0.1.14 (fermer la fenetre garde l'application vivante), le greffon
/// single-instance n'intercepte plus rien : un second lancement posait une seconde icone, et Nolan voyait deux etoiles.
/// On tient le compte nous-memes, avec un verrou nomme qui porte le pid ET la version :
///   - meme version, instance vivante : la doyenne garde la main, on lui demande de montrer sa fenetre, et on s'efface ;
///   - version differente (une mise a jour a ete installee dans le dos) : la neuve prend la place de l'ancienne ;
///   - instance qui ne repond pas en trois secondes : elle est figee, on la remplace — jamais d'application qui refuse
///     de demarrer parce qu'un fantome tient le verrou.
fn fichier_verrou(app: &AppHandle) -> std::path::PathBuf {
    let d = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&d);
    d.join("instance.verrou")
}
fn fichier_appel(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir()).join("montre-toi")
}
/// Le pid est-il encore un Montis ? (un pid recycle par un autre programme ne doit pas nous faire ceder la place)
fn montis_vivant(pid: u32) -> bool {
    #[cfg(windows)]
    let sortie = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output();
    #[cfg(not(windows))]
    let sortie = std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "command="]).output();
    sortie.map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("montis")).unwrap_or(false)
}
fn terminer_instance(pid: u32) {
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status();
    // On demande d'abord poliment (l'instance range ses affaires), puis on tranche : une instance figee ne repond pas
    // a un TERM, et personne ne doit rester avec deux etoiles parce qu'un fantome refuse de mourir.
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if !montis_vivant(pid) { return; }
        }
        let _ = std::process::Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}
fn ecrire_verrou(app: &AppHandle) {
    let _ = std::fs::write(fichier_verrou(app), format!("{} {}", std::process::id(), env!("CARGO_PKG_VERSION")));
}
/// Vrai quand ce processus doit disparaitre sans rien poser : une autre instance a la main.
fn doit_s_effacer(app: &AppHandle) -> bool {
    let nous = std::process::id();
    let contenu = std::fs::read_to_string(fichier_verrou(app)).unwrap_or_default();
    let mut mots = contenu.split_whitespace();
    let pid = mots.next().and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    let version = mots.next().unwrap_or("").to_string();
    if pid == 0 || pid == nous || !montis_vivant(pid) { ecrire_verrou(app); return false; }
    if version == env!("CARGO_PKG_VERSION") {
        let appel = fichier_appel(app);
        let _ = std::fs::write(&appel, nous.to_string());
        journaliser(app, &format!("Montis tourne deja (pid {pid}, v{version}) : on lui demande de se montrer et on s'efface"));
        for _ in 0..15 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if !appel.exists() { return true; }
        }
        journaliser(app, &format!("l'instance {pid} ne repond pas : on la remplace"));
        let _ = std::fs::remove_file(&appel);
    } else {
        journaliser(app, &format!("instance en v{version} (pid {pid}) : elle laisse la place a la v{}", env!("CARGO_PKG_VERSION")));
    }
    terminer_instance(pid);
    std::thread::sleep(std::time::Duration::from_millis(800));
    ecrire_verrou(app);
    false
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
            reglages, enregistrer_reglages, version_coque, plateforme, montrer, masquer, identifiant, poser_jeton, terminer_autorisations, relancer, autorisations_systeme,
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
            if doit_s_effacer(&handle) { std::process::exit(0); }
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
            // FERMER LA FENÊTRE RÉDUIT DANS LA BARRE, L'APPLICATION RESTE (08/09). La règle d'origine — fenêtre fermée =
            // application quittée — tuait le PONT avec la fenêtre : au redémarrage du cœur, la page (503 du relais pendant
            // la relance) fermait la fenêtre d'elle-même, et avec elle les yeux du poste — le PC de Nolan perdait toutes
            // ses actions jusqu'à relance manuelle. Quitter passe par le menu de l'icône (« Quitter Montis ») ; rouvrir,
            // par la même icône ou le Dock.
            if let Some(w) = app.get_webview_window("main") {
                let h3 = handle.clone();
                w.on_window_event(move |e| { if let tauri::WindowEvent::CloseRequested { api, .. } = e { api.prevent_close(); journaliser(&h3, "fenêtre fermée → réduite dans la barre (le pont reste)"); if let Some(f) = h3.get_webview_window("main") { let _ = f.hide(); } } });
            }
            // Un second lancement nous appelle plutot que de poser sa propre icone : on montre la fenetre pour lui.
            { let h6 = handle.clone(); let appel = fichier_appel(&handle); let _ = std::fs::remove_file(&appel);
              tauri::async_runtime::spawn(async move { loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if appel.exists() { let _ = std::fs::remove_file(&appel); journaliser(&h6, "un second lancement nous appelle : fenetre montree"); montrer_fenetre(&h6, false); }
              } }); }
            // MISE À JOUR AUTOMATIQUE : au démarrage puis toutes les QUINZE MINUTES (08/09 ; six heures avant) ; téléchargée,
            // installée, redémarrage. La page de l'interface, elle, se recharge seule quand le cœur la change (PAGE_VERSION) ;
            // l'application ne redémarre qu'à une mise à jour d'elle-même — quinze minutes, c'est le délai au bout duquel
            // une correction publiée est DANS les mains de toute la flotte, sans que personne ne touche à rien. La vérification
            // est une requête légère (latest.json) : quinze minutes ne coûte rien.
            { let h4 = handle.clone(); tauri::async_runtime::spawn(async move { loop {
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                verifier_mise_a_jour(&h4).await;
                tokio::time::sleep(std::time::Duration::from_secs(15 * 60)).await;
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
                    "quitter" => { let _ = std::fs::remove_file(fichier_verrou(app)); app.exit(0) }
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
