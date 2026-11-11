# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Application desktop Windows (Tauri v2, WebView2). Fenêtre sans cadre système de 1080x720, redimensionnable (minimum 760x560) : l'application dessine sa propre barre de titre. Elle doit se comporter en application de bureau, jamais en page web (demande de l'utilisateur). Pas de version mobile.

## Users

Un utilisateur principal : la personne qui prépare les lots elle-même, régulièrement (plusieurs lots par semaine). Elle connaît les fichiers MSISDN et le format attendu ; elle n'a pas besoin d'être guidée pas à pas, mais doit pouvoir vérifier d'un coup d'œil ce qui sera écrit.

## Product Purpose

Transformer une liste de numéros de téléphone (colonne `msisdn` d'un fichier Excel) en fichier CSV prêt à être transmis, une ligne par numéro, par exemple `MSISDN,237653282055,AgentFloatRegion,SouthWest`. Succès : un fichier exact (bon nombre de lignes, bon format, aucun numéro déformé ni dupliqué) produit en quelques clics.

## Positioning

Là où l'on bricolerait le CSV dans Excel (numéros transformés en `2.37E+11`, doublons oubliés, virgules mal échappées), l'outil garantit que l'aperçu affiché est octet pour octet ce qui sera écrit : la même fonction Rust construit l'aperçu et le fichier.

## Operating Context

- Entrée : un classeur `.xlsx` dont la 1re feuille a un en-tête `msisdn` en ligne 1, puis un numéro par ligne (quelques centaines de numéros typiquement, ex. 476).
- Sortie : un CSV UTF-8, fins de ligne `\r\n`, envoyé ensuite à une autre équipe qui l'importe elle-même dans son système.
- Travail de bureau sur poste Windows, en répétition : importer, choisir le type d'en-tête, renseigner ses champs, vérifier l'aperçu, enregistrer.

## Capabilities and Constraints

- Import Excel : nettoyage des espaces, dédoublonnage en gardant l'ordre, numéros numériques jamais en notation scientifique ni avec `.0`.
- Un seul paramètre principal vient du fichier : le numéro (`{{numero}}`, alias `{{parametreprincipal}}`), une ligne générée par numéro.
- Nouveau lot en trois écrans (demande de l'utilisateur, octobre 2026) : 1. bouton d'import au centre, puis « Valider » ; 2. bouton de sélection du type d'en-tête, dont le nom et la 1re ligne s'écrivent en haut ; 3. la grille du fichier où l'on remplit les champs directement (ou dans le volet), avec aperçu en direct puis génération.
- Types d'en-tête : selon l'enjeu, le format change entièrement (3 colonnes pour un type, 30 pour un autre). Chaque type vient avec ses champs, définis cellule par cellule (ex. « IDEnti » à remplir de 0000000, « couleur » de rouge) : sa 1re ligne (vide = aucune, nombre de lignes possible) et les colonnes de chaque ligne. Un champ est saisi une fois et répété partout où il est utilisé. Types enregistrés dans la base, créés et modifiés dans leur onglet.
- Champs facultatifs par défaut, certains absolument obligatoires. Chaque champ a un type de valeur (texte, nombre, date JJ/MM/AAAA, identifiant/code, téléphone), deviné d'après son nom et sa valeur par défaut, modifiable ; la valeur est contrôlée avant d'écrire.
- Aperçu obligatoire avant génération ; génération impossible sans fichier chargé, tant qu'un champ obligatoire est vide ou qu'une valeur ne correspond pas à son type.
- Dossier de sortie choisi une fois et mémorisé ; « Générer » écrit directement un fichier daté dedans, sans écraser l'existant.
- Historique local (base SQLite) de tous les lots générés, CSV complet compris : taille de la base affichée, recherche d'un numéro, re-téléchargement, suppression.
- Toute la logique métier est côté Rust ; l'interface appelle des commandes via `invoke`.
- Interface en français.

## Brand Commitments

- Style **Excel / Power Query**, thème **sombre par défaut et blanc au choix** (préférence confirmée en octobre 2026, remplace la direction « poste de contrôle MoMo » sur fond clair).
- **Couleurs MoMo absolument** (confirmé) : bleu MoMo `#004F71`, jaune `#FFCB05`, dans les deux thèmes ; pas de vert Excel.
- **Application de bureau pure** (confirmé) : une seule barre de titre, pas de double en-tête, aucune allure de page web.
- Logo fourni : `src/assets/logo-momo.png` (512x512), à utiliser **tel quel**, pastille « New » comprise ; il sert aussi d'icône de l'application.
- Ce qui rendrait le résultat raté (confirmé) : trop chargé ou décoratif ; trop austère (formulaire gris sans hiérarchie) ; flou sur ce qui sera écrit dans le fichier ; trop de clics.

## Evidence on Hand

- Fichier de test réel : `MoMo_Advance_Pro_Users_ToBeCanceled.xlsx` (476 numéros), hors dépôt.
- Logo MoMo fourni (`src/assets/logo-momo.png`). Pas de charte graphique complète : ne pas inventer d'autres éléments de marque.

## Product Principles

1. Ce qu'on voit est ce qui sera écrit : l'aperçu est la vérité du fichier.
2. Exactitude avant vitesse, vitesse juste après : le moins de clics possible pour un lot correct.
3. Les chiffres qui comptent (numéros chargés, doublons retirés, lignes écrites) sont toujours visibles.
4. Erreurs dites clairement, en français, avec la marche à suivre.
