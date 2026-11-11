---
name: MMC Batch
description: Application de bureau Windows aux couleurs MoMo, en langage Power Query, qui transforme une liste de numéros Excel en fichier CSV.
colors:
  momo-navy: "#004f71"
  momo-yellow: "#ffcb05"
  momo-yellow-hover-dark: "#ffd63a"
  momo-yellow-hover-light: "#f2bf00"
  momo-yellow-press: "#e3b300"
  on-yellow: "#00293b"
  dark-titlebar: "#04121a"
  dark-ribbon: "#0a1922"
  dark-canvas: "#0b1820"
  dark-surface: "#0f1f28"
  dark-surface-2: "#142731"
  dark-sheet: "#0d1b23"
  dark-head: "#12242e"
  dark-grid: "#1d3540"
  dark-border: "#22404c"
  dark-border-strong: "#3b5c6a"
  dark-text: "#e5eef2"
  dark-text-2: "#a7bac4"
  dark-text-3: "#85a0ab"
  dark-link: "#86cbeb"
  dark-file-text: "#ffd94f"
  dark-input-text: "#a9dcf4"
  dark-bad-text: "#ff9c92"
  light-canvas: "#eef2f5"
  light-surface: "#ffffff"
  light-surface-2: "#f3f6f8"
  light-grid: "#dfe6eb"
  light-border: "#d5dfe6"
  light-border-strong: "#a9bbc7"
  light-text: "#13222b"
  light-text-2: "#42535e"
  light-text-3: "#5d6f7a"
  light-file-fill: "#fff5cc"
  light-file-line: "#e2b300"
  light-file-text: "#00405c"
  light-input-fill: "#e1eef6"
  light-tint-50: "#e9f1f5"
  light-tint-100: "#d3e3eb"
  light-bad-fill: "#fdeceb"
  light-bad-text: "#b42318"
  light-danger-line: "#f2c4bf"
  light-disabled-fill: "#e7edf1"
  light-selection: "#ffe27a"
  caption-close: "#c42b1c"
  caption-close-press: "#b22a1b"
typography:
  window-title:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
  tab:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 600
  body:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.4
    fontFeature: "tnum"
  cell:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: "26px"
  label:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
  caps:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 600
    letterSpacing: "0.06em"
  section:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "13.5px"
    fontWeight: 600
  dialog-title:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
  bytes:
    fontFamily: "Cascadia Mono, Consolas, Courier New, monospace"
    fontSize: "12.5px"
    fontWeight: 400
  fx-mark:
    fontFamily: "Cambria, Georgia, serif"
    fontSize: "14px"
    fontWeight: 400
rounded:
  hairline: "3px"
  control: "4px"
  panel: "6px"
  pill: "999px"
spacing:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "18px"
components:
  titlebar:
    backgroundColor: "{colors.dark-titlebar}"
    textColor: "{colors.dark-text}"
    height: "36px"
  caption-button:
    width: "46px"
    height: "36px"
  ribbon:
    backgroundColor: "{colors.dark-ribbon}"
    height: "40px"
  fx-bar:
    backgroundColor: "{colors.dark-ribbon}"
    height: "28px"
  button-primary:
    backgroundColor: "{colors.momo-yellow}"
    textColor: "{colors.on-yellow}"
    rounded: "{rounded.control}"
    height: "28px"
  button-secondary:
    backgroundColor: "{colors.dark-surface}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.control}"
    height: "28px"
  input:
    backgroundColor: "{colors.dark-sheet}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.control}"
    height: "28px"
  grid-cell:
    backgroundColor: "{colors.dark-sheet}"
    height: "26px"
  status-bar:
    backgroundColor: "{colors.dark-ribbon}"
    height: "24px"
---

# Design System: MMC Batch

## Overview

**Creative North Star : « La requête Power Query aux couleurs MoMo »**

Une application de bureau Windows, pas une page web dans une fenêtre. Le lot se prépare comme une requête Power Query : une source, un type d'en-tête, des champs, chaque étape inscrite dans le volet « Étapes appliquées », et une grille qui montre les octets exacts du fichier. Sombre par défaut (graphite teinté du bleu MoMo), thème blanc au choix depuis la barre de titre (choix mémorisé). Les couleurs MoMo sont imposées par l'utilisateur : bleu nuit pour le cadre, jaune pour l'action et la sélection.

Rejeté explicitement : l'allure de page web (double barre de titre, curseur main, grands titres centrés, liens soulignés, colonne centrée), le formulaire de réglages à côté d'un aperçu, la syntaxe `{{…}}` à taper pour créer un champ.

**Key Characteristics :**
- Fenêtre sans cadre système : une seule barre (36px) qui sert de barre de titre (glisser pour déplacer, double-clic pour agrandir), porte le logo, les onglets, la bascule de thème et les boutons de fenêtre Windows 11.
- Grille façon Power Query : lettres de colonne, type de la valeur (ABC, 1²3, ID, date, téléphone), numéros de ligne figés.
- Trois styles de cellule seulement, comme les styles d'Excel : jaune pâle = valeur du fichier, bleu = valeur saisie, rouge = champ manquant ou valeur à corriger.
- Jaune plein = l'action principale de l'écran et l'anneau de la cellule sélectionnée (bleu MoMo en thème blanc).
- Plat : filets d'1px ; seule ombre, celle des menus et du petit panneau d'action posé sur la feuille vide.

## Colors

Deux couleurs de marque (bleu MoMo `#004f71`, jaune MoMo `#ffcb05`) sur des neutres teintés de bleu. Deux jeux de jetons dans `src/styles.css` : `:root` / `[data-theme="dark"]` (défaut) et `[data-theme="light"]` ; les composants n'utilisent que les noms sémantiques.

### Rôles
- **Cadre** : barre de titre `--titlebar` (sombre #04121a, blanc #004f71 bleu MoMo), filet jaune 2px dessous ; ruban, barre fx et barre d'état `--ribbon`.
- **Canevas et feuille** : `--bg` derrière la grille, `--sheet` dans les cellules, `--head` pour les en-têtes de lignes et colonnes, filets `--grid`.
- **Volets et menus** : `--surface`, survol `--tint-50`, sélection `--tint-100`.
- **Action** : `--accent` jaune MoMo, texte `--on-accent` bleu nuit ; survol plus clair en sombre, plus foncé en blanc.
- **Anneau** : `--ring` jaune (sombre) ou bleu MoMo (blanc) : cellule sélectionnée, focus clavier.
- **Texte interactif** : `--link` bleu MoMo éclairci (#86cbeb) en sombre, bleu MoMo en blanc.
- **Styles de cellule** : `--file-*` (fichier), `--input-*` (saisie), `--bad-*` (manquant, invalide).

### Named Rules
**The Three Cell Styles Rule.** Une cellule n'a que trois teintes possibles en plus du neutre : fichier (jaune pâle), saisie (bleu), à revoir (rouge). Elles s'appliquent à toute la cellule quand la valeur l'occupe entière, au morceau sinon (`CRPLMT_‹numéro›@10000`).

**The Yellow Is Action Rule.** Le jaune plein n'habille que l'action principale de l'écran (Importer, Valider, Générer, Enregistrer), l'onglet actif, l'étape faite et l'anneau de sélection en sombre. Jamais de texte jaune sur fond clair.

**The Never Color Alone Rule.** Un état se lit aussi en mots : « Obligatoire », « deviné », « modifié », `‹Région›`, « À renseigner : … ».

## Typography

**Interface :** Segoe UI Variable Text (repli Segoe UI). **Octets :** Cascadia Mono (repli Consolas), réservé à ce qui sera écrit tel quel (barre fx, nom du fichier, modèle, cellules de texte fixe de l'éditeur). Le « fx » de la barre de formule est en Cambria italique, comme dans Excel.

### Hierarchy
- **Titre de fenêtre** (600, 12px) ; **onglets** (600, 12.5px).
- **Corps** (400, 13px, chiffres tabulaires) ; **cellules** (12.5px, lignes de 26px).
- **Libellés** (600, 12px) ; **petites capitales** (600, 11px, +0.06em) pour les en-têtes de volet et la barre d'état.
- **Rubriques** (600, 13.5px) ; **titre de panneau** (600, 14px). Aucun titre plus grand : c'est une application, pas une page.

## Layout

Fenêtre de 1080×720 par défaut (minimum 760×560). De haut en bas : barre de titre 36px (+ filet 2px), ruban 40px (commandes de l'écran, action suivante à droite), barre fx 28px (zone Nom 120–200px + « fx » + contenu), bandeau de message éventuel, puis canevas et volet « Paramètres de la requête » (`clamp(236px, 28%, 300px)`), barre d'état 24px. Le canevas et le volet défilent seuls ; tout le reste garde sa hauteur.

## Components

### Barre de titre
Logo 22px tel quel, « MMC Batch », onglets (soulignement jaune 3px sur l'onglet actif, compteur de lots en pastille), zone libre de déplacement, bascule soleil/lune, puis les trois boutons de légende Windows 11 (46px de large, glyphes de 10px au trait d'1px ; Fermer passe au rouge #c42b1c au survol).

### Nouveau lot (trois écrans)
1. **Source** : feuille vide estompée, et au centre un petit panneau (400px, bord, ombre de menu) avec « Importer un fichier » en jaune. Une fois lu : la grille des premières lignes (numéros en jaune pâle), feuille et « 1re ligne = noms » dans le ruban, « Valider » à droite.
2. **Type d'en-tête** : panneau central « Sélectionner le type d'en-tête » (liste qui s'ouvre vers le haut si la place manque). Le type choisi s'écrit en haut : son nom dans la zone Nom, sa 1re ligne dans la barre fx ; la grille montre l'en-tête et les premières lignes.
3. **Champs** : la grille du fichier, éditable. Une cellule bleue se modifie sur place (double-clic, Entrée, F2 ou frappe directe ; Entrée descend comme dans Excel ; flèches et Tab pour se déplacer) et toute la colonne suit. La cellule choisie affiche sa formule dans la barre fx (« D5 {{region}} »). Le volet liste les champs : type de la valeur (menu, « deviné » tant qu'il n'est pas choisi), pastille Obligatoire / Facultatif, saisie, contrôle de la valeur. Changer le type ou l'obligation s'enregistre dans le type d'en-tête.

### Grille (signature)
En-têtes de colonne sur une ligne : lettre, pictogramme de type, nom. Numéros de ligne figés à gauche, le 1 de l'en-tête en jaune. Cellule sélectionnée : anneau 2px `--ring`, en-têtes de sa ligne et de sa colonne teintés. **Recopie vers le bas** : quand une valeur change, ses cellules s'allument de jaune l'une après l'autre (14ms par ligne, 700ms), une seule fois ; neutralisé si `prefers-reduced-motion`.

### Éditeur de types (signature)
Liste des types à gauche (marques « modifié » / « nouveau »). À droite, la ligne d'en-tête et les colonnes de chaque ligne en **rangées de cellules** (A1, B1… puis A, B…). Une cellule choisie ouvre son inspecteur juste dessous : contenu en boutons segmentés (Texte fixe, Champ à remplir, Numéro du fichier ou Nombre de lignes, Formule), puis selon le cas le texte, ou le nom du champ (ex. IDEnti), le type de la valeur, la valeur par défaut (ex. 0000000), Obligatoire. Tableau « Champs à remplir » dessous. Pied : modèle du fichier construit par Rust, Enregistrer (Ctrl+S), Utiliser pour un lot, Dupliquer, et Supprimer isolé par un vide (second clic « Confirmer »).

### Messages
Bandeau sous la barre fx, comme les bandeaux de Power Query : erreur (rouge), réussite et information (bleu clair), à faire (neutre, icône rouge). La barre d'état résume en petites capitales (« 2 champs à revoir », « 477 lignes · 4 colonnes · UTF-8 · CRLF »).

## Do's and Don'ts

### Do:
- **Do** garder le comportement d'application de bureau : curseur flèche, texte d'interface non sélectionnable (seuls les champs, la barre fx et le modèle le sont), pas de menu contextuel ni de raccourcis du navigateur.
- **Do** utiliser le logo MoMo tel quel et les deux couleurs MoMo, dans les deux thèmes.
- **Do** écrire les états en mots en plus de la couleur.
- **Do** garder les survols courts (80–90ms) et une seule animation signature (recopie vers le bas).

### Don't:
- **Don't** remettre une barre de titre système au-dessus de la barre de l'application.
- **Don't** utiliser le curseur main, des liens soulignés ou de grands titres centrés.
- **Don't** demander de taper `{{…}}` pour créer un champ : l'inspecteur de cellule le fait (la syntaxe reste pour les formules).
- **Don't** ajouter d'autres teintes de cellule que fichier, saisie, à revoir.
