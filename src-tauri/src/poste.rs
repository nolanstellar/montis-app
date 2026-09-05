//! LES MAINS DE MONTIS SUR CE POSTE. Chaque commande est appelée par le cœur (via l'interface) avec une entrée
//! structurée et rend un résultat lisible. Rien n'est deviné : une action impossible sur ce système le dit tel quel.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

fn maison() -> PathBuf { dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")) }

/// Un chemin donné par la personne : « ~/Documents », relatif au dossier personnel, ou absolu.
fn resoudre(chemin: &str) -> PathBuf {
    let c = chemin.trim();
    if let Some(reste) = c.strip_prefix("~/") { return maison().join(reste); }
    if c == "~" { return maison(); }
    let p = PathBuf::from(c);
    if p.is_absolute() { p } else { maison().join(p) }
}

/// Périmètre : le dossier personnel et les volumes/disques ; jamais le système.
fn dans_le_perimetre(p: &Path) -> bool {
    let m = maison();
    if p.starts_with(&m) { return true; }
    #[cfg(target_os = "macos")] { if p.starts_with("/Volumes") || p.starts_with("/Users/Shared") { return true; } }
    #[cfg(target_os = "windows")] {
        let s = p.to_string_lossy().to_lowercase();
        if !(s.starts_with("c:\\windows") || s.starts_with("c:\\program files") || s.starts_with("c:\\programdata")) && p.is_absolute() { return true; }
    }
    false
}

fn normaliser(s: &str) -> String {
    s.chars().map(|c| match c { 'à' | 'â' | 'ä' => 'a', 'é' | 'è' | 'ê' | 'ë' => 'e', 'î' | 'ï' => 'i', 'ô' | 'ö' => 'o', 'ù' | 'û' | 'ü' => 'u', 'ç' => 'c', c => c.to_ascii_lowercase() }).collect()
}

fn shell(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd).args(args).output().map_err(|e| format!("{cmd} : {e}"))?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()) }
    else { Err(format!("{cmd} : {}", String::from_utf8_lossy(&out.stderr).trim())) }
}
#[cfg(target_os = "windows")]
fn powershell(script: &str) -> Result<String, String> { shell("powershell", &["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script]) }
#[cfg(target_os = "macos")]
fn osascript(script: &str) -> Result<String, String> { shell("osascript", &["-e", script]) }

// ---------------------------------------------------------------- ouvrir : adresse web, fichier, dossier, application

#[tauri::command]
pub fn ouvrir_cible(cible: String) -> Result<String, String> {
    let c = cible.trim();
    if c.starts_with("http://") || c.starts_with("https://") || c.contains("://") {
        #[cfg(target_os = "macos")] shell("open", &[c])?;
        #[cfg(target_os = "windows")] shell("cmd", &["/c", "start", "", c])?;
        return Ok(format!("Ouvert : {c}"));
    }
    let p = resoudre(c);
    if p.exists() {
        if !dans_le_perimetre(&p) { return Err(format!("« {c} » est hors du périmètre autorisé (dossier personnel, disques).")); }
        #[cfg(target_os = "macos")] shell("open", &[p.to_str().unwrap_or(c)])?;
        #[cfg(target_os = "windows")] shell("cmd", &["/c", "start", "", p.to_str().unwrap_or(c)])?;
        return Ok(format!("Ouvert : {}", p.display()));
    }
    // Une application, par son nom.
    application("lancer".into(), c.to_string(), None)
}

// ---------------------------------------------------------------- applications

/// Les noms qu'on dit en français → le nom que le système connaît.
fn nom_systeme(nom: &str) -> String {
    let n = normaliser(nom.trim());
    let table: &[(&str, &str, &str)] = &[
        ("calculette", "Calculator", "calc"), ("calculatrice", "Calculator", "calc"), ("mail", "Mail", "outlook"), ("courrier", "Mail", "outlook"), ("navigateur", "Safari", "msedge"),
        ("notes", "Notes", "notepad"), ("bloc-notes", "TextEdit", "notepad"), ("bloc notes", "TextEdit", "notepad"), ("rappels", "Reminders", "outlook"), ("calendrier", "Calendar", "outlook"), ("agenda", "Calendar", "outlook"),
        ("musique", "Music", "wmplayer"), ("photos", "Photos", "ms-photos:"), ("messages", "Messages", "ms-chat:"), ("reglages systeme", "System Settings", "ms-settings:"), ("preferences systeme", "System Settings", "ms-settings:"), ("parametres", "System Settings", "ms-settings:"),
        ("apercu", "Preview", "mspaint"), ("terminal", "Terminal", "wt"), ("finder", "Finder", "explorer"), ("explorateur", "Finder", "explorer"), ("fichiers", "Finder", "explorer"), ("chrome", "Google Chrome", "chrome"), ("word", "Microsoft Word", "winword"), ("excel", "Microsoft Excel", "excel"), ("powerpoint", "Microsoft PowerPoint", "powerpnt"), ("teams", "Microsoft Teams", "ms-teams:"), ("spotify", "Spotify", "spotify:"), ("whatsapp", "WhatsApp", "whatsapp:"), ("visual studio code", "Visual Studio Code", "code"), ("vscode", "Visual Studio Code", "code"),
    ];
    for (fr, mac, win) in table { if n == *fr { return if cfg!(target_os = "macos") { mac.to_string() } else { win.to_string() }; } }
    nom.trim().to_string()
}

#[tauri::command]
pub fn application(action: String, nom: String, fichier: Option<String>) -> Result<String, String> {
    let nom = nom_systeme(&nom);
    let n = nom.as_str();
    match action.as_str() {
        "lancer" | "ouvrir" => {
            #[cfg(target_os = "macos")] {
                match fichier { Some(f) => shell("open", &["-a", n, resoudre(&f).to_str().unwrap_or(&f)])?, None => shell("open", &["-a", n])? };
            }
            #[cfg(target_os = "windows")] {
                let script = match fichier { Some(f) => format!("Start-Process -FilePath '{}' -ArgumentList '\"{}\"'", n.replace('\'', "''"), resoudre(&f).display()), None => format!("Start-Process -FilePath '{}'", n.replace('\'', "''")) };
                powershell(&script).or_else(|_| shell("cmd", &["/c", "start", "", n]))?;
            }
            Ok(format!("{n} est lancé."))
        }
        "fermer" | "quitter" => {
            #[cfg(target_os = "macos")] osascript(&format!("tell application \"{}\" to quit", n.replace('"', "")))?;
            #[cfg(target_os = "windows")] powershell(&format!("Get-Process | Where-Object {{ $_.ProcessName -like '*{}*' -or $_.MainWindowTitle -like '*{}*' }} | ForEach-Object {{ $_.CloseMainWindow() | Out-Null }}", n.replace('\'', "''"), n.replace('\'', "''")))?;
            Ok(format!("{n} est fermé."))
        }
        "basculer" | "activer" => {
            #[cfg(target_os = "macos")] osascript(&format!("tell application \"{}\" to activate", n.replace('"', "")))?;
            #[cfg(target_os = "windows")] powershell(&format!("$s = New-Object -ComObject WScript.Shell; if (-not $s.AppActivate('{}')) {{ throw 'fenêtre introuvable' }}", n.replace('\'', "''")))?;
            Ok(format!("{n} est au premier plan."))
        }
        "liste" => {
            #[cfg(target_os = "macos")] { let r = osascript("tell application \"System Events\" to get name of every process whose background only is false")?; return Ok(r); }
            #[cfg(target_os = "windows")] { let r = powershell("Get-Process | Where-Object { $_.MainWindowTitle } | Select-Object -ExpandProperty ProcessName -Unique | Sort-Object")?; return Ok(r.replace("\r\n", ", ")); }
            #[allow(unreachable_code)] Ok(String::new())
        }
        a => Err(format!("action « {a} » inconnue (lancer, fermer, basculer, liste).")),
    }
}

// ---------------------------------------------------------------- système

#[tauri::command]
pub fn capture_ecran(app: AppHandle, fenetre: Option<String>) -> Result<String, String> {
    use xcap::{Monitor, Window};
    let bureau = dirs::desktop_dir().unwrap_or_else(maison);
    let nom = format!("Capture Montis {}.png", chrono::Local::now().format("%Y-%m-%d %H.%M.%S"));
    let chemin = bureau.join(&nom);
    let image = match fenetre {
        Some(f) if !f.trim().is_empty() => {
            let voulu = normaliser(&f);
            let w = Window::all().map_err(|e| e.to_string())?.into_iter().find(|w| normaliser(&w.title().unwrap_or_default()).contains(&voulu) || normaliser(&w.app_name().unwrap_or_default()).contains(&voulu)).ok_or_else(|| format!("aucune fenêtre « {f} » à l'écran"))?;
            w.capture_image().map_err(|e| format!("capture de la fenêtre : {e} (sur Mac : autoriser l'enregistrement d'écran dans Réglages Système › Confidentialité)"))?
        }
        _ => {
            let m = Monitor::all().map_err(|e| e.to_string())?.into_iter().find(|m| m.is_primary().unwrap_or(false)).ok_or("aucun écran")?;
            m.capture_image().map_err(|e| format!("capture : {e} (sur Mac : autoriser l'enregistrement d'écran dans Réglages Système › Confidentialité)"))?
        }
    };
    image.save(&chemin).map_err(|e| e.to_string())?;
    let _ = app.notification().builder().title("Montis").body(format!("Capture enregistrée sur le Bureau : {nom}")).show();
    Ok(chemin.display().to_string())
}

#[tauri::command]
pub fn presse_papiers_lire(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn presse_papiers_ecrire(app: AppHandle, texte: String) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(texte).map_err(|e| e.to_string())?;
    Ok("Copié dans le presse-papiers.".into())
}

/// Volume : « 0-100 », « +15 », « -15 », « muet », « son ».
#[tauri::command]
pub fn regler_volume(valeur: String) -> Result<String, String> {
    let v = valeur.trim().to_lowercase();
    #[cfg(target_os = "macos")] {
        if v == "muet" || v == "mute" { osascript("set volume with output muted")?; return Ok("Son coupé.".into()); }
        if v == "son" || v == "unmute" { osascript("set volume without output muted")?; return Ok("Son rétabli.".into()); }
        let actuel: i32 = osascript("output volume of (get volume settings)")?.parse().unwrap_or(50);
        let cible = if let Some(d) = v.strip_prefix('+') { actuel + d.parse::<i32>().unwrap_or(15) } else if let Some(d) = v.strip_prefix('-') { actuel - d.parse::<i32>().unwrap_or(15) } else { v.parse::<i32>().map_err(|_| "niveau attendu : 0 à 100, +15, -15, muet, son")? };
        let cible = cible.clamp(0, 100);
        osascript(&format!("set volume output volume {cible}"))?;
        return Ok(format!("Volume à {cible} %."));
    }
    #[cfg(target_os = "windows")] {
        // Sans dépendance : les touches multimédia (volume par pas de 2 %), le muet par la touche dédiée.
        let touche = |code: u8, n: u32| powershell(&format!("$s = New-Object -ComObject WScript.Shell; 1..{n} | ForEach-Object {{ $s.SendKeys([char]{code}) }}"));
        if v == "muet" || v == "son" || v == "mute" || v == "unmute" { touche(173, 1)?; return Ok(if v.starts_with('m') { "Son coupé.".into() } else { "Son rétabli.".into() }); }
        if let Some(d) = v.strip_prefix('+') { let pas = d.parse::<u32>().unwrap_or(15) / 2; touche(175, pas.max(1))?; return Ok(format!("Volume monté de {} %.", pas * 2)); }
        if let Some(d) = v.strip_prefix('-') { let pas = d.parse::<u32>().unwrap_or(15) / 2; touche(174, pas.max(1))?; return Ok(format!("Volume baissé de {} %.", pas * 2)); }
        let cible: u32 = v.parse().map_err(|_| "niveau attendu : 0 à 100, +15, -15, muet, son")?;
        touche(174, 50)?; touche(175, (cible.min(100) / 2).max(0))?;
        return Ok(format!("Volume à environ {} %.", cible.min(100)));
    }
    #[allow(unreachable_code)] Err("volume non pris en charge sur ce système".into())
}

#[tauri::command]
pub fn regler_luminosite(valeur: String) -> Result<String, String> {
    #[cfg(target_os = "windows")] {
        let n: u32 = valeur.trim().parse().map_err(|_| "niveau attendu : 0 à 100")?;
        powershell(&format!("(Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightnessMethods).WmiSetBrightness(1,{})", n.min(100)))
            .map_err(|_| "cet écran ne permet pas le réglage logiciel de la luminosité (écrans externes)".to_string())?;
        return Ok(format!("Luminosité à {} %.", n.min(100)));
    }
    #[cfg(target_os = "macos")] {
        let _ = valeur;
        // macOS n'expose pas la luminosité sans outil tiers : on utilise les touches si la personne le veut, sinon on le dit.
        return Err("sur Mac, la luminosité ne se règle pas par logiciel sans outil tiers : utilise les touches F1/F2 ou le Centre de contrôle".into());
    }
    #[allow(unreachable_code)] Err("non pris en charge".into())
}

#[tauri::command]
pub fn verrouiller() -> Result<String, String> {
    #[cfg(target_os = "macos")] shell("osascript", &["-e", "tell application \"System Events\" to keystroke \"q\" using {command down, control down}"]).or_else(|_| shell("pmset", &["displaysleepnow"]))?;
    #[cfg(target_os = "windows")] shell("rundll32.exe", &["user32.dll,LockWorkStation"])?;
    Ok("Poste verrouillé.".into())
}

#[tauri::command]
pub fn mettre_en_veille(ecran_seulement: Option<bool>) -> Result<String, String> {
    let ecran = ecran_seulement.unwrap_or(true);
    #[cfg(target_os = "macos")] { if ecran { shell("pmset", &["displaysleepnow"])?; } else { osascript("tell application \"System Events\" to sleep")?; } }
    #[cfg(target_os = "windows")] { if ecran { powershell("(Add-Type '[DllImport(\"user32.dll\")]public static extern int SendMessage(int hWnd, int hMsg, int wParam, int lParam);' -Name a -PassThru)::SendMessage(-1, 0x0112, 0xF170, 2)")?; } else { shell("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"])?; } }
    Ok(if ecran { "Écran en veille.".into() } else { "Mise en veille.".into() })
}

/// Impression : fichier, copies, imprimante nommée (sinon celle par défaut). `file: "file"` ouvre la file d'impression.
#[tauri::command]
pub fn imprimer(fichier: String, copies: Option<u32>, imprimante: Option<String>) -> Result<String, String> {
    let n = copies.unwrap_or(1).clamp(1, 20);
    if fichier.trim() == "file" || fichier.trim() == "file d'attente" {
        #[cfg(target_os = "macos")] shell("open", &["/System/Library/CoreServices/Applications/Print Center.app"]).or_else(|_| shell("open", &["-b", "com.apple.print.PrintCenter"]))?;
        #[cfg(target_os = "windows")] shell("cmd", &["/c", "start", "", "ms-settings:printers"])?;
        return Ok("File d'impression ouverte.".into());
    }
    let p = resoudre(&fichier);
    if !p.exists() { return Err(format!("« {fichier} » n'existe pas.")); }
    #[cfg(target_os = "macos")] {
        let copies_s = format!("-#{n}");
        let mut args: Vec<&str> = vec![&copies_s];
        let imp; if let Some(i) = &imprimante { imp = i.clone(); args.push("-P"); args.push(&imp); }
        let chemin = p.to_string_lossy().to_string(); args.push(&chemin);
        shell("lpr", &args)?;
    }
    #[cfg(target_os = "windows")] {
        let chemin = p.display().to_string().replace('\'', "''");
        let script = match &imprimante {
            Some(i) => format!("1..{n} | ForEach-Object {{ Start-Process -FilePath '{chemin}' -Verb PrintTo -ArgumentList '\"{}\"' -PassThru | Wait-Process -Timeout 60 }}", i.replace('\'', "''")),
            None => format!("1..{n} | ForEach-Object {{ Start-Process -FilePath '{chemin}' -Verb Print -PassThru | Wait-Process -Timeout 60 }}"),
        };
        powershell(&script)?;
    }
    Ok(format!("Impression lancée : {} ({} exemplaire{}){}.", p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), n, if n > 1 { "s" } else { "" }, imprimante.map(|i| format!(" sur {i}")).unwrap_or_default()))
}

#[derive(Serialize)]
pub struct Infos { systeme: String, machine: String, processeur: String, memoire_totale_go: f64, memoire_libre_go: f64, disque_libre_go: f64, batterie: Option<String>, utilisateur: String }

#[tauri::command]
pub fn infos_systeme() -> Infos {
    use sysinfo::{Disks, System};
    let mut s = System::new(); s.refresh_memory(); s.refresh_cpu_all();
    let disques = Disks::new_with_refreshed_list();
    let libre = disques.list().iter().filter(|d| d.mount_point() == Path::new("/") || d.mount_point().to_string_lossy().starts_with("C:")).map(|d| d.available_space()).max().unwrap_or(0);
    let batterie = { #[cfg(target_os = "macos")] { shell("pmset", &["-g", "batt"]).ok().and_then(|o| o.lines().nth(1).map(|l| l.trim().to_string())) } #[cfg(target_os = "windows")] { powershell("(Get-WmiObject Win32_Battery).EstimatedChargeRemaining").ok().filter(|s| !s.is_empty()).map(|s| format!("{s} %")) } #[cfg(not(any(target_os = "macos", target_os = "windows")))] { None } };
    Infos {
        systeme: format!("{} {}", System::name().unwrap_or_default(), System::os_version().unwrap_or_default()),
        machine: System::host_name().unwrap_or_default(),
        processeur: s.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default(),
        memoire_totale_go: (s.total_memory() as f64 / 1e9 * 10.0).round() / 10.0,
        memoire_libre_go: (s.available_memory() as f64 / 1e9 * 10.0).round() / 10.0,
        disque_libre_go: (libre as f64 / 1e9 * 10.0).round() / 10.0,
        batterie,
        utilisateur: whoami(),
    }
}
fn whoami() -> String { std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default() }

// ---------------------------------------------------------------- fichiers

const IGNORES: &[&str] = &["node_modules", ".git", "Library", ".Trash", ".npm", ".cache", ".vscode", "AppData", "Windows", "Program Files", "Program Files (x86)", "$Recycle.Bin", "ProgramData", ".ollama", ".cargo", ".rustup", "DerivedData"];

/// Recherche par le nom (tous les mots doivent figurer, accents et casse ignorés), dans le dossier personnel ou un dossier donné.
#[tauri::command]
pub fn chercher_fichiers(requete: String, dossier: Option<String>, maximum: Option<usize>) -> Result<Vec<String>, String> {
    let racine = dossier.filter(|d| !d.trim().is_empty()).map(|d| resoudre(&d)).unwrap_or_else(maison);
    if !dans_le_perimetre(&racine) { return Err("dossier hors du périmètre autorisé".into()); }
    let mots: Vec<String> = normaliser(&requete).split(|c: char| !c.is_alphanumeric()).filter(|m| m.len() >= 2 && !["le", "la", "les", "de", "du", "des", "un", "une", "mon", "ma", "mes", "fichier", "dossier", "document"].contains(m)).map(String::from).collect();
    if mots.is_empty() { return Err("donne quelques mots du nom à chercher".into()); }
    let max = maximum.unwrap_or(20).min(100);
    let debut = std::time::Instant::now();
    let mut trouves: Vec<String> = Vec::new();
    // Bureau, Documents, Téléchargements d'abord (c'est là que vivent les devis), puis le reste du dossier personnel.
    let mut racines: Vec<PathBuf> = Vec::new();
    if racine == maison() { for d in [dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir()].into_iter().flatten() { if d.exists() { racines.push(d); } } }
    racines.push(racine.clone());
    for r in racines {
        for entree in walkdir::WalkDir::new(&r).follow_links(false).max_depth(8).into_iter().filter_entry(|e| !e.file_name().to_str().map(|n| IGNORES.contains(&n) || (n.starts_with('.') && e.depth() > 0)).unwrap_or(false)) {
            if debut.elapsed().as_secs() > 12 || trouves.len() >= max { break; }
            let Ok(e) = entree else { continue };
            let nom = normaliser(&e.file_name().to_string_lossy());
            let chemin = e.path().display().to_string();
            if mots.iter().all(|m| nom.contains(m)) && !trouves.contains(&chemin) { trouves.push(chemin); }
        }
        if trouves.len() >= max || debut.elapsed().as_secs() > 12 { break; }
    }
    Ok(trouves)
}

#[tauri::command]
pub fn lister_dossier(dossier: Option<String>) -> Result<Vec<String>, String> {
    let d = dossier.filter(|x| !x.trim().is_empty()).map(|x| resoudre(&x)).unwrap_or_else(maison);
    if !dans_le_perimetre(&d) { return Err("hors du périmètre".into()); }
    let mut sortie: Vec<String> = std::fs::read_dir(&d).map_err(|e| e.to_string())?.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).map(|e| format!("{}{}", e.file_name().to_string_lossy(), if e.path().is_dir() { "/" } else { "" })).collect();
    sortie.sort(); sortie.truncate(80);
    Ok(sortie)
}

/// Texte d'un fichier : texte, code, CSV, JSON, HTML, PDF (couche texte). Tronqué à `maximum` caractères.
#[tauri::command]
pub fn lire_fichier(chemin: String, maximum: Option<usize>) -> Result<String, String> {
    let p = resoudre(&chemin);
    if !dans_le_perimetre(&p) { return Err("hors du périmètre autorisé".into()); }
    if !p.is_file() { return Err(format!("« {chemin} » n'existe pas ou n'est pas un fichier")); }
    let max = maximum.unwrap_or(6000).min(60_000);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let contenu = if ext == "pdf" {
        let octets = std::fs::read(&p).map_err(|e| e.to_string())?;
        pdf_extract::extract_text_from_mem(&octets).map_err(|e| format!("PDF illisible : {e}"))?
    } else if ["png", "jpg", "jpeg", "gif", "webp", "heic", "mp4", "mov", "mp3", "zip", "dmg", "exe", "app", "docx", "xlsx", "pptx"].contains(&ext.as_str()) {
        return Err(format!("un fichier .{ext} ne se lit pas comme du texte : je peux l'ouvrir"));
    } else {
        String::from_utf8_lossy(&std::fs::read(&p).map_err(|e| e.to_string())?).to_string()
    };
    let contenu = contenu.trim();
    if contenu.is_empty() { return Err("fichier vide, ou PDF sans couche texte (scan) : je peux l'ouvrir".into()); }
    Ok(format!("{} ({} caractères{}) :\n{}", p.display(), contenu.chars().count(), if contenu.chars().count() > max { format!(", tronqué à {max}") } else { String::new() }, contenu.chars().take(max).collect::<String>()))
}

#[tauri::command]
pub fn creer_fichier(chemin: String, contenu: Option<String>, ecraser: Option<bool>) -> Result<String, String> {
    let p = resoudre(&chemin);
    if !dans_le_perimetre(&p) { return Err("hors du périmètre autorisé".into()); }
    if p.exists() && !ecraser.unwrap_or(false) { return Err(format!("« {} » existe déjà : écraser demande une confirmation explicite", p.display())); }
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    std::fs::write(&p, contenu.unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(format!("Créé : {}", p.display()))
}

#[tauri::command]
pub fn renommer_fichier(chemin: String, nouveau_nom: String) -> Result<String, String> {
    let p = resoudre(&chemin);
    if !dans_le_perimetre(&p) || !p.exists() { return Err("fichier introuvable ou hors périmètre".into()); }
    let cible = p.parent().unwrap_or(Path::new(".")).join(nouveau_nom.trim());
    if cible.exists() { return Err(format!("« {} » existe déjà", cible.display())); }
    std::fs::rename(&p, &cible).map_err(|e| e.to_string())?;
    Ok(format!("Renommé : {}", cible.display()))
}

#[tauri::command]
pub fn deplacer_fichier(chemin: String, dossier_cible: String) -> Result<String, String> {
    let p = resoudre(&chemin); let d = resoudre(&dossier_cible);
    if !dans_le_perimetre(&p) || !p.exists() { return Err("fichier introuvable ou hors périmètre".into()); }
    if !dans_le_perimetre(&d) { return Err("dossier cible hors périmètre".into()); }
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let cible = d.join(p.file_name().ok_or("nom invalide")?);
    if cible.exists() { return Err(format!("« {} » existe déjà dans le dossier cible", cible.display())); }
    std::fs::rename(&p, &cible).or_else(|_| std::fs::copy(&p, &cible).and_then(|_| std::fs::remove_file(&p))).map_err(|e| e.to_string())?;
    Ok(format!("Déplacé : {}", cible.display()))
}

// ---------------------------------------------------------------- fenêtres

/// « déplacer » (x, y), « redimensionner » (l, h), « gauche » / « droite » (moitié d'écran), « plein » (maximiser), « réduire ».
#[tauri::command]
pub fn fenetre(action: String, application: Option<String>, x: Option<i32>, y: Option<i32>, largeur: Option<i32>, hauteur: Option<i32>) -> Result<String, String> {
    let app = application.unwrap_or_default();
    #[cfg(target_os = "macos")] {
        let cible = if app.trim().is_empty() { "first application process whose frontmost is true".to_string() } else { format!("application process \"{}\"", app.replace('"', "")) };
        let script = match action.as_str() {
            "deplacer" => format!("tell application \"System Events\" to set position of window 1 of ({cible}) to {{{}, {}}}", x.unwrap_or(0), y.unwrap_or(0)),
            "redimensionner" => format!("tell application \"System Events\" to set size of window 1 of ({cible}) to {{{}, {}}}", largeur.unwrap_or(1000), hauteur.unwrap_or(700)),
            "gauche" | "droite" | "plein" => {
                let bounds = osascript("tell application \"Finder\" to get bounds of window of desktop")?; // « 0, 0, L, H »
                let v: Vec<i32> = bounds.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                let (l, h) = (v.get(2).copied().unwrap_or(1440), v.get(3).copied().unwrap_or(900) - 25);
                let (px, pl) = match action.as_str() { "gauche" => (0, l / 2), "droite" => (l / 2, l / 2), _ => (0, l) };
                format!("tell application \"System Events\" to tell window 1 of ({cible})\nset position to {{{px}, 25}}\nset size to {{{pl}, {h}}}\nend tell")
            }
            "reduire" => format!("tell application \"System Events\" to set value of attribute \"AXMinimized\" of window 1 of ({cible}) to true"),
            a => return Err(format!("action « {a} » inconnue (deplacer, redimensionner, gauche, droite, plein, reduire)")),
        };
        osascript(&script).map_err(|e| format!("{e} — sur Mac, cette action demande l'autorisation Accessibilité (Réglages Système › Confidentialité et sécurité › Accessibilité › Montis)"))?;
        return Ok("Fenêtre placée.".into());
    }
    #[cfg(target_os = "windows")] {
        let sel = if app.trim().is_empty() { "$h = (Add-Type -MemberDefinition '[DllImport(\"user32.dll\")] public static extern IntPtr GetForegroundWindow();' -Name fg -PassThru)::GetForegroundWindow()".to_string() } else { format!("$p = Get-Process | Where-Object {{ $_.MainWindowHandle -ne 0 -and ($_.ProcessName -like '*{0}*' -or $_.MainWindowTitle -like '*{0}*') }} | Select-Object -First 1; if (-not $p) {{ throw 'fenêtre introuvable' }}; $h = $p.MainWindowHandle", app.replace('\'', "''")) };
        let def = "$u = Add-Type -MemberDefinition '[DllImport(\"user32.dll\")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r); [DllImport(\"user32.dll\")] public static extern bool ShowWindow(IntPtr h, int c);' -Name win -PassThru; $sw = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea";
        let corps = match action.as_str() {
            "deplacer" => format!("$u::MoveWindow($h, {}, {}, 1000, 700, $true)", x.unwrap_or(0), y.unwrap_or(0)),
            "redimensionner" => format!("$u::MoveWindow($h, 40, 40, {}, {}, $true)", largeur.unwrap_or(1000), hauteur.unwrap_or(700)),
            "gauche" => "$u::ShowWindow($h, 9); $u::MoveWindow($h, $sw.X, $sw.Y, [int]($sw.Width/2), $sw.Height, $true)".into(),
            "droite" => "$u::ShowWindow($h, 9); $u::MoveWindow($h, $sw.X + [int]($sw.Width/2), $sw.Y, [int]($sw.Width/2), $sw.Height, $true)".into(),
            "plein" => "$u::ShowWindow($h, 3)".into(),
            "reduire" => "$u::ShowWindow($h, 6)".into(),
            a => return Err(format!("action « {a} » inconnue")),
        };
        powershell(&format!("Add-Type -AssemblyName System.Windows.Forms; {sel}; {def}; {corps} | Out-Null"))?;
        return Ok("Fenêtre placée.".into());
    }
    #[allow(unreachable_code)] Err("non pris en charge".into())
}

// ---------------------------------------------------------------- messages (Mac : l'application Messages envoie iMessage ou SMS via l'iPhone)

/// Envoie un message par l'application Messages du Mac : iMessage si le correspondant y est, sinon SMS relayé par l'iPhone
/// (Réglages iPhone › Messages › Transfert de SMS). Demande une fois l'autorisation « Automatisation » pour Messages.
#[tauri::command]
pub fn envoyer_message(destinataire: String, texte: String) -> Result<String, String> {
    #[cfg(target_os = "macos")] {
        let mut d = destinataire.trim().replace([' ', '.', '-'], "");
        if d.starts_with('0') && d.len() == 10 { d = format!("+33{}", &d[1..]); }
        let t = texte.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(r#"tell application "Messages"
    set leTexte to "{t}"
    set cible to "{d}"
    try
        set svc to 1st account whose service type = iMessage and enabled is true
        send leTexte to participant cible of svc
        return "iMessage"
    on error
        set svc2 to 1st account whose service type = SMS and enabled is true
        send leTexte to participant cible of svc2
        return "SMS"
    end try
end tell"#);
        let via = osascript(&script).map_err(|e| format!("{e} — Messages n'a pas pu envoyer (autorisation Automatisation refusée, ou transfert de SMS non activé sur l'iPhone)"))?;
        return Ok(format!("Message envoyé à {destinataire} par {via}."));
    }
    #[cfg(not(target_os = "macos"))] { let _ = (destinataire, texte); Err("l'envoi de messages n'existe que sur Mac (application Messages) ; depuis un PC, pas de SMS".into()) }
}

// ---------------------------------------------------------------- notification système

#[tauri::command]
pub fn notifier(app: AppHandle, titre: Option<String>, texte: String) -> Result<(), String> {
    app.notification().builder().title(titre.unwrap_or_else(|| "Montis".into())).body(texte).show().map_err(|e| e.to_string())
}
