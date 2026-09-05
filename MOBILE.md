# Montis sur iPhone et Android — ce qui sera possible, ce que le système interdit

Le projet est structuré pour produire les versions mobiles avec Tauri 2 (`npx tauri ios init`, `npx tauri android init`) sur la
même interface web et le même cœur. Avant de le promettre à un client, voici ce que chaque plateforme permet réellement.

| Fonction | iPhone (app native) | Android (app native) | Aujourd'hui (page web) |
|---|---|---|---|
| Parler et entendre Montis, écran de conversation | oui | oui | oui |
| Mot d'activation, écoute permanente écran allumé | oui, application ouverte | oui | oui, page ouverte |
| Écoute permanente écran verrouillé / en arrière-plan | **non** (Apple coupe le micro d'une app en arrière-plan, sauf mode audio continu très encadré) | partiel (service au premier plan avec notification permanente) | non |
| Raccourci d'appel global | **non** (pas de raccourci clavier ; Siri → App Intents « Dis à Montis… » possible) | partiel (bouton d'action, tuile) | non |
| Notifications (rappels, rendez-vous, conflits) | oui | oui | oui si ajouté à l'écran d'accueil |
| Ouvrir Waze / Plans avec l'itinéraire | oui | oui | oui |
| Composer un appel | oui, avec la confirmation iOS (un appui) | oui, direct | oui, avec confirmation |
| Envoyer un SMS / WhatsApp sans appui | **non** (l'envoi exige le geste de l'utilisateur) | oui (autorisation SMS, une fois) | non |
| Lire les SMS, appels, notifications des autres apps | **non, interdit à toute app** | oui (autorisations dédiées) | non |
| Contacts, calendrier, rappels du téléphone | oui (une autorisation, une fois) | oui | non |
| Fichiers du téléphone | partiel (dossier de l'app + sélecteur) | oui | non |
| Photos | oui (autorisation) | oui | non |
| Capture d'écran d'une autre app, contrôle d'autres apps | **non** | partiel (service d'accessibilité) | non |
| Presse-papiers | lecture avec un appui, écriture libre | oui | idem |
| Impression | oui (AirPrint) | oui | non |
| Mises à jour automatiques | non : par l'App Store ou TestFlight | oui (hors magasin) ou Play Store | immédiates (page servie par le cœur) |

Publication : **App Store** exige un compte développeur Apple (99 €/an), une revue, une politique de confidentialité en ligne,
et refuse une app qui n'est qu'une coquille web sans valeur native : Montis passera si les fonctions natives (notifications,
Siri, contacts) sont réelles. **Google Play** : compte développeur (25 $ une fois), revue plus légère ; la distribution hors
magasin par fichier `.apk` est possible pour des clients professionnels.

Recommandation : garder la page web pour le téléphone tant que les fonctions ci-dessus ne sont pas indispensables, et
livrer l'application mobile quand le Raccourci iOS (rappels, agenda, iMessage) ou Siri deviennent un argument de vente.
