# MMC Batch — Excel vers CSV

Petite application Windows qui transforme la colonne `msisdn` d'un fichier Excel en fichier CSV, une ligne par numéro :

```
MSISDN,237653282055,AgentFloatRegion,SouthWest
```

- **Backend Rust** (`src-tauri/`) : lecture Excel (`calamine`), nettoyage, dédoublonnage, construction des lignes, écriture du CSV et historique des lots (SQLite).
- **Frontend Vue 3** (`src/`) : affiche l'interface et appelle les commandes Rust via `invoke`.

Stack : Tauri 2.12, `tauri-plugin-dialog` 2.8, Vue 3.5, Vite 8, calamine 0.36, rusqlite 0.40 (SQLite embarqué).

## Prérequis (Windows)

1. [Node.js](https://nodejs.org/) (LTS ou plus récent)
2. [Rust](https://www.rust-lang.org/tools/install) via `rustup` (toolchain `stable-x86_64-pc-windows-msvc`)
3. Visual Studio Build Tools avec la charge de travail **« Développement Desktop en C++ »** (MSVC)
4. WebView2 (déjà présent sur Windows 10/11 à jour)

## Installation

```bash
npm install
```

## Lancement (développement)

```bash
npm run tauri dev
```

## Build (installeur Windows)

```bash
npm run tauri build
```

Les installeurs sont générés dans `src-tauri/target/release/bundle/` (`msi/` et `nsis/`).

## Tests

```bash
cd src-tauri
cargo test
```

Test de bout en bout sur un vrai classeur (ignoré par défaut) :

```powershell
cd src-tauri
$env:MMC_XLSX = "C:\chemin\vers\fichier.xlsx"
cargo test fichier_reel -- --ignored --nocapture
```

## Utilisation

L'application est une fenêtre de bureau sans cadre système : sa barre du haut porte les onglets, la bascule **thème sombre / blanc** (mémorisée) et les boutons réduire, agrandir, fermer ; on la saisit pour déplacer la fenêtre, double-clic pour l'agrandir. Le style suit Power Query (ruban, barre fx, grille, volet « Étapes appliquées », barre d'état) aux couleurs MoMo.

### Nouveau lot, en trois écrans

1. **Source** (Ctrl+O) : bouton « Importer un fichier » au centre. Excel (au choix de la feuille) ou CSV ; sa colonne `msisdn` (sinon la 1re colonne de numéros) donne le **paramètre principal** : une ligne du fichier généré par numéro. Les cellules vides sont ignorées, les espaces supprimés, les doublons retirés (l'ordre est conservé). La grille montre les premières lignes ; « Valider » (Ctrl+Entrée) passe à l'écran suivant.
2. **Type d'en-tête** : bouton « Sélectionner le type d'en-tête » au centre. Le type choisi **s'écrit en haut** : son nom dans la zone Nom, sa 1re ligne dans la barre fx ; la grille montre l'en-tête et les premières lignes. Le dernier type choisi est retenu. « Valider ».
3. **Champs** : la grille du fichier à générer (40 premières lignes), éditable. On remplit un champ directement dans une de ses cellules bleues (double-clic, Entrée, F2 ou frappe directe ; Entrée descend, flèches et Tab pour se déplacer) : toute la colonne suit. Le volet « Paramètres de la requête » liste aussi les champs : valeur, **type de la valeur** (deviné, modifiable), **Obligatoire / Facultatif**. Ces deux réglages s'enregistrent dans le type d'en-tête. Une cellule choisie affiche sa formule dans la barre fx.

Couleurs des cellules : jaune pâle = valeur du fichier, bleu = valeur saisie, rouge = champ obligatoire vide (`‹Région›`) ou valeur qui ne correspond pas à son type. Un bandeau sous la barre fx dit ce qui reste à faire. La même fonction Rust construit la grille et le fichier.

- **Dossier de sortie** : bouton dossier du ruban (écran 3), mémorisé (par défaut `Documents\MMC Batch`).
- **Générer le CSV** (Ctrl+S) : écrit directement dans le dossier de sortie un fichier daté `export_AAAA-MM-JJ_HHhMM.csv`, sans jamais écraser un fichier existant (sinon suffixe ` (2)`). Le fichier est en UTF-8, avec des fins de ligne `\r\n` et l'échappement CSV standard (une valeur saisie qui contient une virgule ou un guillemet est échappée, dans l'en-tête comme sur les lignes). Refusé tant qu'un champ obligatoire est vide ou qu'une valeur est hors type. Le lot est ajouté à l'historique avec le nom de son type ; « Nouveau lot » repart de l'écran 1.

### Types d'en-tête (onglet)

Chaque type vient avec ses champs. Il se décrit **cellule par cellule** : la **ligne d'en-tête** (A1, B1… ; aucune cellule = pas d'en-tête) et les **colonnes de chaque ligne** (A, B…). On clique une cellule pour choisir son contenu :

- **Texte fixe**, écrit tel quel ;
- **Champ à remplir** : un nom (ex. `IDEnti`, `couleur`), un **type de valeur** (texte, nombre, date JJ/MM/AAAA, identifiant/code, téléphone ; deviné d'après le nom et la valeur par défaut), **Obligatoire** ou non (facultatif par défaut), une **valeur par défaut** (ex. `0000000`, `rouge`). Deux cellules au même nom partagent la même valeur ;
- **Numéro du fichier** (colonnes ; au moins une colonne le contient) ou **Nombre de lignes** (en-tête) ;
- **Formule** : texte mêlé de champs, ex. `CRPLMT_{{numero}}@{{montant}}`.

Le tableau « Champs à remplir » reprend tous les champs (libellé affiché, type, obligatoire, valeur par défaut). « Saisir en une ligne » découpe une ligne CSV collée (pratique pour 30 colonnes) ; « Depuis le fichier chargé » crée un type à partir d'un CSV déjà fini. Le pied montre le modèle du fichier construit par Rust (`‹numéro›`, `‹IDEnti›`…). Ctrl+S enregistre ; les modifications non enregistrées sont gardées (marque « modifié ») en changeant de type ou d'onglet. Stockage : en-tête et colonnes en texte avec balises `{{…}}` (majuscules, accents, espaces et `_` ignorés).

Exemple (type « AgentFloatRegion », fourni à la création de la base) :

```
En-tête  : HDR,UpdateIndividualRatingParameter,Init AgentFloatRegion {{region}},{{extId}},{{total}}, AgentFloatRegion init,1
Colonnes : MSISDN | {{numero}} | AgentFloatRegion | {{region}}
Champs   : Région (obligatoire), Identifiant externe (obligatoire)
→ HDR,UpdateIndividualRatingParameter,Init AgentFloatRegion SouthWest,ExtId12340,476, AgentFloatRegion init,1
  MSISDN,237653282055,AgentFloatRegion,SouthWest
```

Les trois types fournis (AgentFloatRegion, CRPLMT, EleCashback) sont tirés des fichiers réels : à ajuster ou supprimer.

### Historique (onglet)

Un journal des lots groupés par jour, le plus récent ouvert avec ses premières lignes réelles. La recherche d'un numéro ouvre les lots qui le contiennent et montre la ligne exacte où il se trouve. Chaque lot se re-télécharge dans le dossier de sortie (fichier identique octet pour octet) ou se supprime (second clic « Confirmer »).

## Base de données (historique)

- Fichier : `%APPDATA%\com.mmc.batchvf\historique.sqlite`. Son chemin exact est affiché en bas de l'onglet Historique.
- Contenu : pour chaque lot, la date, le fichier Excel source, le type d'en-tête, le nom et le chemin du CSV, le nombre de lignes, l'en-tête, la ligne modèle et le CSV complet. Table `header_types` : les types d'en-tête (description, en-tête, colonnes et champs en JSON).
- Version du schéma dans `PRAGMA user_version` : une base existante est mise à niveau à l'ouverture (table des types garnie des exemples, colonne `type_name` ajoutée aux lots).
- Taille mesurée : environ **25 Ko par lot de 476 numéros**, soit environ 2,5 Mo pour 100 lots et 25 Mo pour 1 000 lots (environ 54 octets par numéro). La taille actuelle est affichée dans l'onglet Historique. Supprimer un lot réduit le fichier (`auto_vacuum`).
- Supprimer un lot de l'historique ne supprime pas le CSV déjà écrit sur le disque.

## Organisation du code

| Fichier | Rôle |
|---|---|
| `src-tauri/src/source.rs` | Lecture Excel ou CSV, colonne `msisdn`, conversion des cellules, dédoublonnage, ligne HDR |
| `src-tauri/src/csv_builder.rs` | `build_line` (partagée aperçu/modèle/fichier), champs renseignés (`Param`), en-tête, échappement, écriture en flux, nom de fichier libre |
| `src-tauri/src/header_types.rs` | Types d'en-tête : champs déduits des balises, validation, exemples fournis |
| `src-tauri/src/db.rs` | SQLite : historique (enregistrement en flux, liste, recherche, re-téléchargement, suppression, taille) et types d'en-tête |
| `src-tauri/src/lib.rs` | Commandes Tauri `read_file`, `build_preview`, `default_output_dir`, `generate_csv`, `list_batches`, `batch_lines`, `db_info`, `export_batch`, `delete_batch`, `list_types`, `save_type`, `delete_type` |
| `src-tauri/capabilities/default.json` | Permissions : boîte de dialogue Ouvrir, et barre de titre de l'application (déplacer, réduire, agrandir, fermer) |
| `src/App.vue` | Coquille : barre de titre unique (onglets, thème, boutons de fenêtre), onglets gardés en mémoire |
| `src/components/BatchView.vue` | Nouveau lot en trois écrans : ruban, barre fx, grille, volet « Étapes appliquées », barre d'état, génération |
| `src/components/SheetGrid.vue` | Grille façon Power Query : sélection, saisie sur place, recopie vers le bas |
| `src/components/TypesView.vue` | Onglet Types d'en-tête : cellules, inspecteur, champs, modèle du fichier |
| `src/components/KindMenu.vue`, `KindGlyph.vue` | Type de la valeur d'un champ (menu et pictogramme) |
| `src/components/WindowControls.vue` | Boutons de fenêtre Windows 11 |
| `src/components/HistoryView.vue` | Onglet Historique (journal par jour) |
| `src/csvDisplay.js` | Balises, cellules et champs d'un type, devinette et contrôle des valeurs (miroir de Rust), chemins courts |
| `src/components/AppIcon.vue` | Pictogrammes |
| `src/styles.css` | Jetons des thèmes sombre et blanc (couleurs MoMo `#004F71`, `#FFCB05`), comportement d'application de bureau, styles de base |
| `src/assets/logo-momo.png` | Logo MoMo (aussi source des icônes de `src-tauri/icons/`) |

Pour régénérer les icônes après un changement de logo : `npx tauri icon src/assets/logo-momo.png` (supprimer ensuite les dossiers `android/` et `ios/` créés dans `src-tauri/icons/`, inutiles ici).
