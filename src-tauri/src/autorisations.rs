//! LES AUTORISATIONS SYSTÈME. Au premier lancement, la coque les demande toutes, une phrase par autorisation, et vérifie
//! elle-même ce qui est accordé — sans attendre que la première action échoue. macOS : Accessibilité, Enregistrement d'écran,
//! Automatisation (System Events, Finder, Messages), Micro, Notifications, dossiers (Bureau, Documents, Téléchargements).
//! Windows : rien à demander au système, sauf le micro (à la première écoute) et les notifications.

use serde::Serialize;
use std::process::Command;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

#[derive(Serialize, Clone)]
pub struct Autorisation {
    pub id: String,
    pub titre: String,
    pub pourquoi: String,
    /// accordee · a_accorder · inconnue (le système ne le dit pas sans demander) · sans_objet
    pub etat: String,
    /// Le panneau des Réglages Système où l'accorder à la main, s'il existe.
    pub reglage: Option<String>,
}

#[cfg(target_os = "macos")]
mod mac {
    use std::os::raw::c_void;
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        pub fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGPreflightScreenCaptureAccess() -> bool;
        pub fn CGRequestScreenCaptureAccess() -> bool;
    }
    pub fn accessibilite() -> bool { unsafe { AXIsProcessTrusted() } }
    pub fn demander_accessibilite() -> bool {
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;
        let cle = CFString::new("AXTrustedCheckOptionPrompt");
        let dict = CFDictionary::from_CFType_pairs(&[(cle.as_CFType(), CFBoolean::true_value().as_CFType())]);
        unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const c_void) }
    }
    pub fn ecran() -> bool { unsafe { CGPreflightScreenCaptureAccess() } }
    pub fn demander_ecran() -> bool { unsafe { CGRequestScreenCaptureAccess() } }
}

fn osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript").args(["-e", script]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

fn reglage(pane: &str) -> Option<String> {
    if cfg!(target_os = "macos") { Some(format!("x-apple.systempreferences:com.apple.preference.security?{pane}")) } else { None }
}

fn fichier_accordees(app: &AppHandle) -> std::path::PathBuf { app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir()).join("autorisations.json") }
fn accordees(app: &AppHandle) -> Vec<String> { std::fs::read_to_string(fichier_accordees(app)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() }
fn retenir(app: &AppHandle, id: &str) { let mut v = accordees(app); if !v.iter().any(|x| x == id) { v.push(id.to_string()); let _ = std::fs::write(fichier_accordees(app), serde_json::to_string(&v).unwrap_or_default()); } }

#[tauri::command]
pub fn etat_autorisations(app: AppHandle) -> Vec<Autorisation> {
    let deja = accordees(&app);
    let mut liste = etat_brut();
    // Ce que le système ne dit pas sans demander (automatisation, micro, notifications, dossiers) : on garde ce que la personne a accordé.
    for a in liste.iter_mut() { if a.etat == "inconnue" && deja.iter().any(|d| d == &a.id) { a.etat = "accordee".into(); } }
    liste
}

fn etat_brut() -> Vec<Autorisation> {
    let a = |id: &str, titre: &str, pourquoi: &str, etat: &str, pane: Option<&str>| Autorisation { id: id.into(), titre: titre.into(), pourquoi: pourquoi.into(), etat: etat.into(), reglage: pane.and_then(reglage) };
    #[cfg(target_os = "macos")] {
        return vec![
            a("accessibilite", "Accessibilité", "Pour déplacer, redimensionner et organiser les fenêtres des autres applications.", if mac::accessibilite() { "accordee" } else { "a_accorder" }, Some("Privacy_Accessibility")),
            a("ecran", "Enregistrement d'écran", "Pour prendre une capture d'écran ou d'une fenêtre quand tu le demandes.", if mac::ecran() { "accordee" } else { "a_accorder" }, Some("Privacy_ScreenCapture")),
            a("automatisation", "Automatisation (System Events, Finder)", "Pour le volume, la veille, les applications, la liste des fenêtres.", "inconnue", Some("Privacy_Automation")),
            a("messages", "Automatisation (Messages)", "Pour envoyer un iMessage ou un SMS via ton iPhone quand tu dis « envoie à… ». Messages s'ouvrira une fois.", "inconnue", Some("Privacy_Automation")),
            a("micro", "Microphone", "Pour t'entendre. Sans lui, pas de voix.", "inconnue", Some("Privacy_Microphone")),
            a("notifications", "Notifications", "Pour te prévenir d'un rappel ou d'un rendez-vous quand Montis n'est pas devant toi.", "inconnue", Some("Notifications")),
            a("dossiers", "Bureau, Documents, Téléchargements", "Pour retrouver, lire, créer et ouvrir tes fichiers.", "inconnue", Some("Privacy_FilesAndFolders")),
        ];
    }
    #[cfg(target_os = "windows")] {
        return vec![
            a("micro", "Microphone", "Pour t'entendre. Windows le demande à la première écoute.", "inconnue", None),
            a("notifications", "Notifications", "Pour te prévenir d'un rappel ou d'un rendez-vous.", "inconnue", None),
            a("systeme", "Fichiers, applications, capture, impression, fenêtres", "Rien à accorder : Windows laisse une application installée agir sur la session de l'utilisateur.", "accordee", None),
        ];
    }
    #[allow(unreachable_code)] vec![a("systeme", "Système", "Aucune autorisation particulière sur ce système.", "accordee", None)]
}

/// Déclenche la vraie demande du système pour cette autorisation (boîte de dialogue macOS), ou ouvre le bon panneau.
#[tauri::command]
pub fn demander_autorisation(app: AppHandle, id: String) -> Result<String, String> {
    let r = demander(app.clone(), &id)?;
    if r == "accordee" || r == "page" { retenir(&app, &id); }
    Ok(r)
}
fn demander(app: AppHandle, id: &str) -> Result<String, String> {
    match id {
        #[cfg(target_os = "macos")]
        "accessibilite" => { let ok = mac::demander_accessibilite(); if !ok { let _ = Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility").spawn(); } Ok(if ok { "accordee".into() } else { "a_accorder".into() }) }
        #[cfg(target_os = "macos")]
        "ecran" => { let ok = mac::demander_ecran(); Ok(if ok { "accordee".into() } else { "a_accorder".into() }) }
        #[cfg(target_os = "macos")]
        "automatisation" => {
            let r1 = osascript("tell application \"System Events\" to get name of first process");
            let r2 = osascript("tell application \"Finder\" to get name");
            Ok(if r1.is_ok() && r2.is_ok() { "accordee".into() } else { format!("a_accorder : {}", r1.err().or(r2.err()).unwrap_or_default().chars().take(120).collect::<String>()) })
        }
        #[cfg(target_os = "macos")]
        "messages" => { let r = osascript("tell application \"Messages\" to get name"); Ok(if r.is_ok() { "accordee".into() } else { format!("a_accorder : {}", r.err().unwrap_or_default().chars().take(120).collect::<String>()) }) }
        #[cfg(target_os = "macos")]
        "dossiers" => {
            let mut ok = true;
            for d in [dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir()].into_iter().flatten() { if std::fs::read_dir(&d).is_err() { ok = false; } }
            Ok(if ok { "accordee".into() } else { "a_accorder".into() })
        }
        "notifications" => { app.notification().builder().title("Montis").body("Les notifications fonctionnent. Je te préviendrai ici.").show().map_err(|e| e.to_string())?; Ok("accordee".into()) }
        "micro" => Ok("page".into()),   // demandé par la page (getUserMedia) : c'est le navigateur système qui affiche la demande ; retenu comme accordé si la page réussit
        _ => Ok("inconnue".into()),
    }
}

/// Ouvre le panneau des Réglages Système correspondant.
#[tauri::command]
pub fn ouvrir_reglage(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")] { Command::new("open").arg(&url).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "windows")] { let _ = url; Command::new("cmd").args(["/c", "start", "", "ms-settings:privacy-microphone"]).spawn().map_err(|e| e.to_string())?; }
    Ok(())
}
