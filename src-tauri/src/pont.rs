//! LE PONT NATIF. La coque s'abonne elle-même au flux d'événements du cœur (/api/flux), dans un fil Rust, indépendamment de la
//! page : une fenêtre cachée suspend la page web sous macOS, pas ce fil. Chaque action destinée à cet appareil est exécutée ici
//! (poste.rs) et son résultat renvoyé au cœur (/api/action-resultat). Reconnexion automatique ; déclaration de l'appareil à
//! chaque connexion (/api/appareil).

use crate::poste;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;

#[derive(Clone, Default)]
pub struct Liaison {
    pub coeur: String,
    pub appareil: String,
    /// La valeur du cookie de la porte (montis_cle) quand le cœur est joint par le tunnel ; vide en local.
    pub jeton: String,
}
pub struct EtatPont(pub Arc<Mutex<Liaison>>);

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().timeout(None).connect_timeout(Duration::from_secs(10)).user_agent(format!("Montis-coque/{}", env!("CARGO_PKG_VERSION"))).build().expect("client http")
}
fn entete_cookie(l: &Liaison) -> Option<String> { if l.jeton.is_empty() { None } else { Some(format!("montis_cle={}", l.jeton)) } }

fn poster(l: &Liaison, route: &str, corps: Value) -> Result<(), String> {
    let c = client();
    let mut r = c.post(format!("{}{}", l.coeur.trim_end_matches('/'), route)).json(&corps).timeout(Duration::from_secs(15));
    if let Some(ck) = entete_cookie(l) { r = r.header("cookie", ck); }
    r.send().map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
    Ok(())
}

/// Exécute une action venue du cœur et rend (ok, résultat).
fn executer(app: &AppHandle, a: &Value) -> (bool, String) {
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(|x| x.to_string());
    let genre = s("genre").unwrap_or_default();
    let r: Result<String, String> = match genre.as_str() {
        "poste" => {
            let action = s("action").unwrap_or_default(); let c = s("cible"); let v = s("valeur");
            match action.as_str() {
                "capture_ecran" => poste::capture_ecran(app.clone(), c),
                "presse_papiers_lire" => poste::presse_papiers_lire(app.clone()),
                "presse_papiers_ecrire" => poste::presse_papiers_ecrire(app.clone(), c.or(v).unwrap_or_default()),
                "volume" => poste::regler_volume(v.or(c).unwrap_or_else(|| "50".into())),
                "luminosite" => poste::regler_luminosite(v.or(c).unwrap_or_else(|| "70".into())),
                "verrouiller" => poste::verrouiller(),
                "veille" => { let t = v.clone().or(c.clone()).unwrap_or_default().to_lowercase(); poste::mettre_en_veille(Some(!(t.contains("ordinateur") || t.contains("machine") || t.contains("pc") || t.contains("mac") || t.contains("compl")))) }
                "imprimer" => { let copies = v.as_ref().and_then(|x| x.parse::<u32>().ok()); let imprimante = v.filter(|x| x.parse::<u32>().is_err()); poste::imprimer(c.unwrap_or_default(), copies, imprimante) }
                "file_impression" => poste::imprimer("file".into(), None, None),
                "infos_systeme" => { let i = poste::infos_systeme(); Ok(format!("{} sur {}, processeur {}, {} Go de mémoire dont {} libres, {} Go de disque libres{}.", i.systeme, i.machine, i.processeur, i.memoire_totale_go, i.memoire_libre_go, i.disque_libre_go, i.batterie.map(|b| format!(", batterie {}", b.split(';').next().unwrap_or("").replace("-InternalBattery-0 (id=", "").split(')').last().unwrap_or("").trim())).unwrap_or_default())) }
                "application_lancer" => poste::application("lancer".into(), c.unwrap_or_default(), v),
                "application_fermer" => poste::application("fermer".into(), c.unwrap_or_default(), None),
                "application_basculer" => poste::application("basculer".into(), c.unwrap_or_default(), None),
                "applications_ouvertes" => poste::application("liste".into(), String::new(), None),
                "fenetre_gauche" => poste::fenetre("gauche".into(), c, None, None, None, None),
                "fenetre_droite" => poste::fenetre("droite".into(), c, None, None, None, None),
                "fenetre_plein" => poste::fenetre("plein".into(), c, None, None, None, None),
                "fenetre_reduire" => poste::fenetre("reduire".into(), c, None, None, None, None),
                "fenetre_deplacer" => { let n: Vec<i32> = v.unwrap_or_default().split(|ch: char| !ch.is_ascii_digit() && ch != '-').filter_map(|x| x.parse().ok()).collect(); poste::fenetre("deplacer".into(), c, n.first().copied(), n.get(1).copied(), None, None) }
                "fenetre_redimensionner" => { let n: Vec<i32> = v.unwrap_or_default().split(|ch: char| !ch.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect(); poste::fenetre("redimensionner".into(), c, None, None, n.first().copied(), n.get(1).copied()) }
                "creer_fichier" => poste::creer_fichier(c.unwrap_or_default(), v, Some(false)),
                "renommer_fichier" => poste::renommer_fichier(c.unwrap_or_default(), v.unwrap_or_default()),
                "deplacer_fichier" => poste::deplacer_fichier(c.unwrap_or_default(), v.unwrap_or_default()),
                autre => Err(format!("action inconnue « {autre} »")),
            }
        }
        "chercher_fichiers" => poste::chercher_fichiers(s("requete").unwrap_or_default(), s("dossier"), Some(20)).map(|l| if l.is_empty() { "Rien trouvé sur ce poste.".into() } else { format!("{} résultat(s) :\n{}", l.len(), l.iter().map(|x| format!("- {x}")).collect::<Vec<_>>().join("\n")) }),
        "ouvrir_cible" => poste::ouvrir_cible(s("cible").unwrap_or_default()),
        "lire_fichier" => poste::lire_fichier(s("chemin").unwrap_or_default(), a.get("maximum").and_then(|v| v.as_u64()).map(|v| v as usize)),
        "lister_dossier" => poste::lister_dossier(s("dossier")).map(|l| l.join("\n")),
        // Actions d'appareil « simples » (le cœur n'attend pas de résultat) : sur un poste, la carte ou le lien s'ouvre dans le système.
        "naviguer" => poste::ouvrir_cible(a.pointer("/liens/google").and_then(|v| v.as_str()).unwrap_or_default().to_string()),
        "ouvrir_url" => poste::ouvrir_cible(s("url").unwrap_or_default()),
        "appeler" => poste::ouvrir_cible(s("lien").unwrap_or_default()),
        "message" => {
            // Sur Mac, Messages envoie pour de bon ; ailleurs, on ouvre le lien préparé (Messages/WhatsApp) et la personne appuie.
            let r = poste::envoyer_message(s("tel").unwrap_or_default(), s("texte").unwrap_or_default());
            if r.is_ok() { r } else { poste::ouvrir_cible(a.pointer("/liens/sms").and_then(|v| v.as_str()).unwrap_or_default().to_string()).and(r) }
        }
        "message_envoyer" => poste::envoyer_message(s("tel").unwrap_or_default(), s("texte").unwrap_or_default()),
        autre => Err(format!("genre inconnu « {autre} »")),
    };
    match r { Ok(t) => (true, t), Err(e) => (false, e) }
}

/// Le fil du pont : à relancer à chaque changement d'adresse ou de jeton (il relit l'état à chaque reconnexion).
pub fn demarrer(app: AppHandle, etat: Arc<Mutex<Liaison>>) {
    std::thread::spawn(move || {
        let mut generation_vue = String::new();
        let mut dernier_message = String::new();   // le même refus ne s'écrit qu'une fois dans le journal
        loop {
            let l = etat.lock().map(|g| g.clone()).unwrap_or_default();
            if l.coeur.is_empty() || l.appareil.is_empty() { std::thread::sleep(Duration::from_secs(2)); continue; }
            let cle = format!("{}|{}|{}", l.coeur, l.appareil, l.jeton);
            if cle != generation_vue { generation_vue = cle.clone(); }
            // Déclaration (plateforme, version) — le cœur sait qu'un poste natif est là.
            let _ = poster(&l, "/api/appareil", json!({ "appareil": l.appareil, "plateforme": format!("app-{}", std::env::consts::OS), "version": env!("CARGO_PKG_VERSION"), "nom": format!("Montis {}", match std::env::consts::OS { "macos" => "Mac", "windows" => "Windows", o => o }) }));
            let c = client();
            // L'appareil dans l'adresse : le cœur ne diffuse à cette coque que ce qui concerne sa personne.
            let mut req = c.get(format!("{}/api/flux?appareil={}", l.coeur.trim_end_matches('/'), l.appareil)).header("accept", "text/event-stream");
            if let Some(ck) = entete_cookie(&l) { req = req.header("cookie", ck); }
            match req.send() {
                Ok(resp) if resp.status().is_success() => {
                    crate::journaliser(&app, &format!("pont : connecté au flux du cœur ({}){}", l.coeur, if l.jeton.is_empty() { " sans jeton (local)" } else { " avec le jeton de la porte" })); dernier_message.clear();
                    let lecteur = BufReader::new(resp);
                    for ligne in lecteur.lines() {
                        let Ok(ligne) = ligne else { crate::journaliser(&app, "pont : flux coupé, reconnexion"); break };
                        // L'état a changé (autre cœur, autre jeton) : on recommence proprement.
                        if let Ok(g) = etat.lock() { if format!("{}|{}|{}", g.coeur, g.appareil, g.jeton) != cle { break; } }
                        let Some(donnees) = ligne.strip_prefix("data: ") else { continue };
                        let Ok(ev) = serde_json::from_str::<Value>(donnees) else { continue };
                        // Une annonce spontanée du cœur (rappel, rendez-vous, conflit) : notification système, même fenêtre cachée.
                        if ev.get("type").and_then(|t| t.as_str()) == Some("annonce") {
                            if let Some(texte) = ev.get("texte").and_then(|t| t.as_str()) { let _ = poste::notifier(app.clone(), Some("Montis".into()), texte.to_string()); }
                            continue;
                        }
                        if ev.get("type").and_then(|t| t.as_str()) != Some("action") { continue; }
                        let Some(a) = ev.get("action") else { continue };
                        let pour = a.get("appareil").and_then(|v| v.as_str()).unwrap_or("");
                        // Pour moi, ou pour personne en particulier (anticipation) : j'agis. Pour un autre appareil : non.
                        if !pour.is_empty() && pour != l.appareil { continue; }
                        // Chaque action dans son propre fil : une action longue (recherche, dialogue d'autorisation macOS) ne retarde pas les autres.
                        let (app2, l2, a2) = (app.clone(), l.clone(), a.clone());
                        std::thread::spawn(move || {
                            let attendu = a2.get("attendu").and_then(|v| v.as_bool()).unwrap_or(false);
                            let (ok, resultat) = executer(&app2, &a2);
                            let id = a2.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            if attendu { let _ = poster(&l2, "/api/action-resultat", json!({ "id": id, "genre": a2.get("genre"), "ok": ok, "resultat": resultat })); }
                            else { let _ = poster(&l2, "/api/action-faite", json!({ "id": id, "genre": a2.get("genre"), "ok": ok, "detail": resultat, "appareil": l2.appareil })); }
                        });
                    }
                }
                Ok(resp) => { let m = format!("pont : flux refusé par le cœur ({}) — {}", resp.status(), if l.jeton.is_empty() { "il faut passer la porte dans la fenêtre Montis (mot de passe d'entreprise), le pont suivra" } else { "jeton refusé : repasser la porte" }); if m != dernier_message { crate::journaliser(&app, &m); dernier_message = m; } }
                Err(e) => { let m = format!("pont : cœur injoignable : {e}"); if m != dernier_message { crate::journaliser(&app, &m); dernier_message = m; } }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}
