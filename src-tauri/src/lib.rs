//! Commandes Tauri appelées par le frontend via `invoke`.
//! Toute la logique métier vit ici ; Vue ne fait qu'afficher.

mod csv_builder;
mod db;
mod header_types;
mod source;

use csv_builder::{Param, PreviewResult, Template};
use db::{BatchLines, BatchSummary, Db, DbInfo, NewBatch};
use header_types::HeaderType;
use source::{Sheet, SheetSummary};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

/// Nombre de lignes montrées par défaut dans l'aperçu.
const DEFAULT_PREVIEW_LIMIT: usize = 5;
/// Fichier de la base, dans le dossier de données de l'application.
const DB_FILE: &str = "historique.sqlite";

/// Fichier chargé : ses lignes restent ici, l'aperçu et la génération les lisent.
#[derive(Default)]
struct Loaded(Mutex<Option<Arc<Sheet>>>);

impl Loaded {
    fn get(&self) -> Option<Arc<Sheet>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Résultat d'une génération : fichier écrit et lot ajouté à l'historique.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateResult {
    batch_id: i64,
    path: String,
    data_lines: usize,
    total_lines: usize,
}

/// Lit un classeur (feuille `sheet`, sinon la 1re) ou un CSV et le garde pour l'aperçu.
/// `header_row` : la 1re ligne donne-t-elle les noms des colonnes ? (`None` = détection).
/// En cas d'erreur, le fichier précédent reste chargé.
/// `async` : exécutée hors du thread principal pour ne pas figer la fenêtre.
#[tauri::command]
async fn read_file(
    loaded: State<'_, Loaded>,
    path: String,
    sheet: Option<String>,
    header_row: Option<bool>,
) -> Result<SheetSummary, String> {
    let sheet = source::read_file(&path, sheet.as_deref(), header_row)?;
    let summary = sheet.summary();
    *loaded.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(sheet));
    Ok(summary)
}

/// En-tête + premières lignes, exactement comme dans le fichier final.
/// `fields` : les fields du type d'en-tête et leurs valeurs. `model` : ignorer le fichier chargé
/// (ligne modèle d'un type en cours d'édition).
#[tauri::command]
fn build_preview(
    loaded: State<'_, Loaded>,
    header: String,
    include_header: bool,
    fields: Vec<Param>,
    limit: Option<usize>,
    model: Option<bool>,
) -> Result<PreviewResult, String> {
    let limit = limit.unwrap_or(DEFAULT_PREVIEW_LIMIT);
    let sheet = if model.unwrap_or(false) { None } else { loaded.get() };
    csv_builder::preview(sheet.as_deref(), &header, include_header, &fields, limit)
}

/// Dossier de sortie proposé tant que l'utilisateur n'en a pas choisi : Documents\MMC Batch.
#[tauri::command]
fn default_output_dir(app: AppHandle) -> Result<String, String> {
    let docs = app.path().document_dir().map_err(|e| e.to_string())?;
    Ok(docs.join("MMC Batch").display().to_string())
}

/// Écrit le CSV du fichier chargé dans `folder` (sans écraser d'existant) puis l'ajoute à l'historique.
/// Refusé tant qu'un field obligatoire du type est vide.
#[tauri::command]
async fn generate_csv(
    db: State<'_, Db>,
    loaded: State<'_, Loaded>,
    header: String,
    include_header: bool,
    fields: Vec<Param>,
    type_name: Option<String>,
    folder: String,
    file_name: String,
    source_name: String,
) -> Result<GenerateResult, String> {
    let sheet = loaded.get().ok_or("Aucune ligne à exporter : importez d'abord un fichier.")?;
    let template = Template::parse(&fields, &sheet)?;
    // Vérifié avant de créer le dossier : aucun fichier n'est touché.
    csv_builder::check_params(&template, &fields)?;
    let folder = PathBuf::from(folder);
    std::fs::create_dir_all(&folder).map_err(|e| csv_builder::create_error(&folder, &e))?;
    let path = csv_builder::unique_path(&folder, &file_name)?;
    let stats = csv_builder::write_csv(&sheet, &header, include_header, &fields, &path)?;

    // Ligne modèle pour l'historique, ex. MSISDN,‹numéro›,SouthWest,CRPLMT_‹numéro›@10000
    let mut model = String::new();
    csv_builder::build_line(&mut model, &sheet.placeholder_row(), &template);
    let header = csv_builder::header_line(&header, include_header, stats.data_lines);
    let batch = NewBatch {
        source_name: &source_name,
        type_name: type_name.as_deref().filter(|n| !n.trim().is_empty()),
        header: header.as_deref(),
        template: &model,
        data_lines: stats.data_lines,
        total_lines: stats.total_lines,
    };
    let batch_id = db.insert(&batch, &path).map_err(|e| {
        format!("Le fichier a bien été écrit dans «\u{a0}{}\u{a0}», mais pas ajouté à l'historique. {e}", path.display())
    })?;

    Ok(GenerateResult {
        batch_id,
        path: path.display().to_string(),
        data_lines: stats.data_lines,
        total_lines: stats.total_lines,
    })
}

/// Lots enregistrés, du plus récent au plus ancien ; `query` filtre sur un numéro.
#[tauri::command]
async fn list_batches(db: State<'_, Db>, query: String) -> Result<Vec<BatchSummary>, String> {
    db.list(query.trim())
}

/// Premières lignes d'un lot enregistré (+ la ligne contenant `query`, s'il y en a une).
#[tauri::command]
async fn batch_lines(db: State<'_, Db>, id: i64, limit: Option<usize>, query: String) -> Result<BatchLines, String> {
    db.lines(id, limit.unwrap_or(DEFAULT_PREVIEW_LIMIT + 1), query.trim())
}

/// Emplacement, taille et nombre de lots de la base.
#[tauri::command]
async fn db_info(db: State<'_, Db>) -> Result<DbInfo, String> {
    db.info()
}

/// Re-télécharge un lot dans `folder` ; renvoie le chemin écrit.
#[tauri::command]
async fn export_batch(db: State<'_, Db>, id: i64, folder: String) -> Result<String, String> {
    db.export(id, &PathBuf::from(folder)).map(|p| p.display().to_string())
}

/// Supprime un lot de l'historique (le fichier déjà écrit sur le disque n'est pas touché).
#[tauri::command]
async fn delete_batch(db: State<'_, Db>, id: i64) -> Result<DbInfo, String> {
    db.delete(id)?;
    db.info()
}

/// Types d'en-tête enregistrés, par ordre alphabétique.
#[tauri::command]
async fn list_types(db: State<'_, Db>) -> Result<Vec<HeaderType>, String> {
    db.list_types()
}

/// Crée ou remplace un type ; renvoie le type enregistré (refusé s'il est inutilisable).
#[tauri::command]
async fn save_type(db: State<'_, Db>, spec: HeaderType) -> Result<HeaderType, String> {
    db.save_type(spec)
}

#[tauri::command]
async fn delete_type(db: State<'_, Db>, id: i64) -> Result<(), String> {
    db.delete_type(id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Loaded::default())
        .setup(|app| {
            // Base de l'historique : %APPDATA%\com.mmc.batchvf\historique.sqlite
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(Db::open(dir.join(DB_FILE))?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_file,
            build_preview,
            default_output_dir,
            generate_csv,
            list_batches,
            batch_lines,
            db_info,
            export_batch,
            delete_batch,
            list_types,
            save_type,
            delete_type
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use header_types::Source;

    /// Test de bout en bout sur un vrai classeur (ignoré par défaut) :
    /// `$env:MMC_XLSX="C:\...\fichier.xlsx"; cargo test fichier_reel -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn fichier_reel() {
        let xlsx = std::env::var("MMC_XLSX").expect("définir MMC_XLSX");
        let sheet = source::read_file(&xlsx, std::env::var("MMC_FEUILLE").ok().as_deref(), None).unwrap();
        let fixed = |name: &str, text: &str| Param { name: name.into(), source: Source::Formule, formula: text.into(), ..Default::default() };
        let fields = [
            fixed("IdentifierType", "MSISDN"),
            Param { name: "MSISDN".into(), source: Source::Numero, ..Default::default() },
            fixed("Parameter", "AgentFloatRegion"),
            Param { name: "Region".into(), value: "SouthWest".into(), required: true, ..Default::default() },
        ];
        let out = std::env::temp_dir().join("mmc_batch_fichier_reel.csv");
        let res = csv_builder::write_csv(&sheet, "on verra ca apres", true, &fields, &out).unwrap();

        let content = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = content.strip_suffix("\r\n").unwrap().split("\r\n").collect();
        assert_eq!(lines.len(), res.total_lines);
        assert_eq!(lines[0], "on verra ca apres");
        let key = sheet.key.expect("colonne de numéros");
        for (line, row) in lines[1..].iter().zip(&sheet.rows) {
            let number = &row[key];
            assert!(number.chars().all(|c| c.is_ascii_digit()), "numéro suspect : {number}");
            assert_eq!(*line, format!("MSISDN,{number},AgentFloatRegion,SouthWest"));
        }
        println!(
            "lus : {} | doublons retirés : {} | lignes écrites : {} (dont en-tête) | {} octets -> {}",
            sheet.total_read,
            sheet.duplicates_removed,
            res.total_lines,
            content.len(),
            out.display()
        );

        // Taille de l'historique : 10 lots de ce fichier dans une base temporaire.
        let db_path = std::env::temp_dir().join("mmc_batch_taille.sqlite");
        std::fs::remove_file(&db_path).ok();
        let db = Db::open(db_path.clone()).unwrap();
        let batch = NewBatch {
            source_name: "reel.xlsx",
            type_name: None,
            header: Some("on verra ca apres"),
            template: "",
            data_lines: 476,
            total_lines: 477,
        };
        for _ in 0..10 {
            db.insert(&batch, &out).unwrap();
        }
        let info = db.info().unwrap();
        println!("base : {} lots -> {} octets ({} octets par lot)", info.batch_count, info.size_bytes, info.size_bytes / 10);
        drop(db);
        std::fs::remove_file(&db_path).ok();
    }

    /// Reprise d'un fichier quelconque (Excel ou CSV, ignoré par défaut) : ce que l'import propose.
    /// `$env:MMC_FICHIER="C:\...\fichier.csv"; $env:MMC_FEUILLE="Batch 01"; cargo test reprise_fichier -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reprise_fichier() {
        let path = std::env::var("MMC_FICHIER").expect("définir MMC_FICHIER");
        let started = std::time::Instant::now();
        let sheet = source::read_file(&path, std::env::var("MMC_FEUILLE").ok().as_deref(), None).unwrap();
        let summary = sheet.summary();
        println!(
            "lu en {:?} | feuille «{}» sur {} | en-têtes : {} | colonnes : {:?} | numéros : {:?}",
            started.elapsed(),
            summary.sheet,
            summary.sheets.len(),
            summary.header_row,
            summary.columns.iter().map(|c| format!("{}={}", c.letter, c.tag)).collect::<Vec<_>>(),
            sheet.key.map(|k| &sheet.columns[k].letter),
        );
        println!(
            "lignes : {} | doublons retirés : {} | sans numéro : {} | éléments proposés : {:?}",
            summary.count, summary.duplicates_removed, summary.skipped, summary.suggested
        );
        // Comme « Depuis le fichier chargé » : numéro, texte fixe pour une colonne constante, saisie sinon.
        let fields: Vec<Param> = summary
            .suggested
            .iter()
            .enumerate()
            .map(|(i, t)| match t.as_str() {
                "{{numero}}" => Param { name: format!("c{i}"), source: Source::Numero, ..Default::default() },
                t if t.starts_with("{{") => Param { name: format!("c{i}"), ..Default::default() },
                t => Param { name: format!("c{i}"), source: Source::Formule, formula: t.into(), ..Default::default() },
            })
            .collect();
        // L'en-tête repris peut citer des balises : il est montré tel quel.
        let header = summary.batch_header.clone().unwrap_or_default();
        let preview = csv_builder::preview(Some(&sheet), &header, true, &fields, 3).unwrap();
        for line in preview.header.iter().chain(preview.lines.iter().map(|l| &l.text)) {
            println!("  {line}");
        }
    }
}
