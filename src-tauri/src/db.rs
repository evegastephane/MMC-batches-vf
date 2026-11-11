//! Petite base SQLite qui conserve chaque lot généré, CSV complet compris,
//! pour le retrouver, le re-télécharger ou y chercher un numéro,
//! ainsi que les types d'en-tête (formats de lot) de l'utilisateur.

use crate::csv_builder::{create_error, unique_path};
use crate::header_types::{self, HeaderType};
use rusqlite::{params, Connection, Transaction, MAIN_DB};
use serde::Serialize;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const SCHEMA: &str = "
    PRAGMA auto_vacuum = FULL; -- le fichier rétrécit quand on supprime un lot
    CREATE TABLE IF NOT EXISTS batches (
        id          INTEGER PRIMARY KEY,
        created_at  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
        source_name TEXT    NOT NULL, -- fichier Excel d'origine
        file_name   TEXT    NOT NULL, -- nom du CSV écrit
        file_path   TEXT    NOT NULL, -- emplacement du CSV écrit
        data_lines  INTEGER NOT NULL,
        total_lines INTEGER NOT NULL,
        header      TEXT,             -- en-tête écrit (NULL si aucun)
        template    TEXT    NOT NULL, -- ligne modèle, ex. MSISDN,‹numéro›,SouthWest
        content     BLOB    NOT NULL  -- le CSV, octet pour octet
    );";

/// Version 1 du schéma (`PRAGMA user_version`) : types d'en-tête, et type utilisé par chaque lot.
const SCHEMA_V1: &str = "
    CREATE TABLE IF NOT EXISTS header_types (
        id         INTEGER PRIMARY KEY,
        name       TEXT NOT NULL,
        spec       TEXT NOT NULL, -- JSON : description, en-tête, fields
        updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
    );";

/// Base de l'historique, partagée entre les commandes Tauri.
pub struct Db {
    conn: Mutex<Connection>,
    path: PathBuf,
}

/// Métadonnées d'un lot à enregistrer.
pub struct NewBatch<'a> {
    pub source_name: &'a str,
    /// Type d'en-tête utilisé.
    pub type_name: Option<&'a str>,
    pub header: Option<&'a str>,
    pub template: &'a str,
    pub data_lines: usize,
    pub total_lines: usize,
}

/// Un lot tel qu'affiché dans l'historique (sans son contenu).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub id: i64,
    pub created_at: String,
    pub source_name: String,
    pub type_name: Option<String>,
    pub file_name: String,
    pub file_path: String,
    pub data_lines: i64,
    pub total_lines: i64,
    pub header: Option<String>,
    pub template: String,
    pub size_bytes: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbInfo {
    pub path: String,
    pub size_bytes: u64,
    pub batch_count: i64,
    /// Total des numéros envoyés, tous lots confondus.
    pub number_count: i64,
}

/// Premières lignes d'un lot enregistré, et la ligne où se trouve le texte cherché.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchLines {
    pub lines: Vec<String>,
    pub found: Option<FoundLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundLine {
    /// Numéro de ligne dans le fichier (1 = première ligne).
    pub number: usize,
    pub text: String,
}

fn sql_err(e: rusqlite::Error) -> String {
    format!("Base de l'historique\u{a0}: {e}")
}

/// Met la base à jour, marche par marche (`PRAGMA user_version`) ; les lots ne sont jamais touchés.
/// 1 : table des types d'en-tête et colonne `type_name` des lots.
/// 2 : types décrits par leurs fields (avec description) : les anciens types sont remplacés par l'exemple.
fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 2 {
        return Ok(());
    }
    let tx = conn.transaction()?;
    if version < 1 {
        tx.execute_batch(SCHEMA_V1)?;
        let has_type: i64 =
            tx.query_row("SELECT count(*) FROM pragma_table_info('batches') WHERE name = 'type_name'", [], |r| r.get(0))?;
        if has_type == 0 {
            tx.execute_batch("ALTER TABLE batches ADD COLUMN type_name TEXT;")?;
        }
        tx.execute_batch("PRAGMA user_version = 1;")?;
    }
    tx.execute_batch("DELETE FROM header_types;")?;
    for t in header_types::examples() {
        insert_type(&tx, &t)?;
    }
    tx.execute_batch("PRAGMA user_version = 2;")?;
    tx.commit()
}

fn insert_type(tx: &Transaction, t: &HeaderType) -> rusqlite::Result<i64> {
    let spec = serde_json::to_string(t).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    tx.execute("INSERT INTO header_types (name, spec) VALUES (?1, ?2)", params![t.name, spec])?;
    Ok(tx.last_insert_rowid())
}

impl Db {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let mut conn = Connection::open(&path).map_err(sql_err)?;
        conn.execute_batch(SCHEMA).map_err(sql_err)?;
        migrate(&mut conn).map_err(sql_err)?;
        Ok(Self { conn: Mutex::new(conn), path })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // Un verrou « empoisonné » par une panique antérieure reste utilisable : la connexion est intacte.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Enregistre un lot ; le CSV est recopié en flux depuis le fichier écrit.
    pub fn insert(&self, batch: &NewBatch, csv: &Path) -> Result<i64, String> {
        let size = std::fs::metadata(csv).map_err(|e| e.to_string())?.len();
        let file_name = csv.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(sql_err)?;
        tx.execute(
            "INSERT INTO batches (source_name, file_name, file_path, data_lines, total_lines, header, template, content, type_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, zeroblob(?8), ?9)",
            params![
                batch.source_name,
                file_name,
                csv.display().to_string(),
                batch.data_lines as i64,
                batch.total_lines as i64,
                batch.header,
                batch.template,
                size as i64,
                batch.type_name,
            ],
        )
        .map_err(sql_err)?;
        let id = tx.last_insert_rowid();
        {
            let mut blob = tx.blob_open(MAIN_DB, c"batches", c"content", id, false).map_err(sql_err)?;
            let mut source = File::open(csv).map_err(|e| e.to_string())?;
            io::copy(&mut source, &mut blob).map_err(|e| format!("Copie dans l'historique impossible\u{a0}: {e}"))?;
        }
        tx.commit().map_err(sql_err)?;
        Ok(id)
    }

    /// Lots du plus récent au plus ancien ; `query` non vide = lots dont le CSV contient ce texte.
    pub fn list(&self, query: &str) -> Result<Vec<BatchSummary>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, created_at, source_name, file_name, file_path, data_lines, total_lines,
                        header, template, length(content), type_name
                 FROM batches
                 WHERE ?1 = '' OR instr(content, CAST(?1 AS BLOB)) > 0
                 ORDER BY id DESC",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([query], |r| {
                Ok(BatchSummary {
                    id: r.get(0)?,
                    created_at: r.get(1)?,
                    source_name: r.get(2)?,
                    file_name: r.get(3)?,
                    file_path: r.get(4)?,
                    data_lines: r.get(5)?,
                    total_lines: r.get(6)?,
                    header: r.get(7)?,
                    template: r.get(8)?,
                    size_bytes: r.get(9)?,
                    type_name: r.get(10)?,
                })
            })
            .map_err(sql_err)?;
        rows.collect::<Result<_, _>>().map_err(sql_err)
    }

    /// Emplacement, taille sur disque et nombre de lots.
    pub fn info(&self) -> Result<DbInfo, String> {
        let (batch_count, number_count) = self
            .conn()
            .query_row("SELECT count(*), coalesce(sum(data_lines), 0) FROM batches", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(sql_err)?;
        let size_bytes = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        Ok(DbInfo { path: self.path.display().to_string(), size_bytes, batch_count, number_count })
    }

    /// Lit en flux les `limit` premières lignes d'un lot ; si `query` n'est pas vide,
    /// continue jusqu'à la première ligne qui le contient.
    pub fn lines(&self, id: i64, limit: usize, query: &str) -> Result<BatchLines, String> {
        let conn = self.conn();
        let blob = conn
            .blob_open(MAIN_DB, c"batches", c"content", id, true)
            .map_err(|_| "Ce lot n'existe plus dans l'historique.".to_string())?;
        let mut lines = Vec::new();
        let mut found = None;
        for (i, raw) in BufReader::new(blob).split(b'\n').enumerate() {
            let mut bytes = raw.map_err(|e| format!("Lecture du lot impossible\u{a0}: {e}"))?;
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if found.is_none() && !query.is_empty() && text.contains(query) {
                found = Some(FoundLine { number: i + 1, text: text.clone() });
            }
            if i < limit {
                lines.push(text);
            }
            if i + 1 >= limit && (query.is_empty() || found.is_some()) {
                break;
            }
        }
        Ok(BatchLines { lines, found })
    }

    /// Re-télécharge un lot dans `folder`, sans écraser de fichier existant.
    pub fn export(&self, id: i64, folder: &Path) -> Result<PathBuf, String> {
        let conn = self.conn();
        let name: String = conn
            .query_row("SELECT file_name FROM batches WHERE id = ?1", [id], |r| r.get(0))
            .map_err(|_| "Ce lot n'existe plus dans l'historique.".to_string())?;
        std::fs::create_dir_all(folder).map_err(|e| create_error(folder, &e))?;
        let path = unique_path(folder, &name)?;
        let mut blob = conn.blob_open(MAIN_DB, c"batches", c"content", id, true).map_err(sql_err)?;
        let mut out = BufWriter::new(File::create(&path).map_err(|e| create_error(&path, &e))?);
        io::copy(&mut blob, &mut out)
            .and_then(|_| out.flush())
            .map_err(|e| format!("Erreur d'écriture dans «\u{a0}{}\u{a0}»\u{a0}: {e}", path.display()))?;
        Ok(path)
    }

    pub fn delete(&self, id: i64) -> Result<(), String> {
        self.conn().execute("DELETE FROM batches WHERE id = ?1", [id]).map_err(sql_err)?;
        Ok(())
    }

    /// Types d'en-tête, par ordre alphabétique.
    pub fn list_types(&self) -> Result<Vec<HeaderType>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT id, name, spec FROM header_types ORDER BY name COLLATE NOCASE, id")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |r| {
                let spec: String = r.get(2)?;
                let mut t: HeaderType = serde_json::from_str(&spec).unwrap_or_default();
                t.id = Some(r.get(0)?);
                t.name = r.get(1)?;
                t.fill_kinds();
                Ok(t)
            })
            .map_err(sql_err)?;
        rows.collect::<Result<_, _>>().map_err(sql_err)
    }

    /// Crée (sans `id`) ou remplace un type ; renvoie le type tel qu'enregistré.
    pub fn save_type(&self, t: HeaderType) -> Result<HeaderType, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(sql_err)?;
        let taken = {
            let mut stmt = tx.prepare("SELECT name FROM header_types WHERE id IS NOT ?1").map_err(sql_err)?;
            let names = stmt.query_map([t.id], |r| r.get(0)).map_err(sql_err)?;
            names.collect::<Result<Vec<String>, _>>().map_err(sql_err)?
        };
        let mut t = t.normalized(&taken)?;
        match t.id {
            Some(id) => {
                let spec = serde_json::to_string(&t).map_err(|e| e.to_string())?;
                let changed = tx
                    .execute(
                        "UPDATE header_types SET name = ?1, spec = ?2, updated_at = datetime('now', 'localtime') WHERE id = ?3",
                        params![t.name, spec, id],
                    )
                    .map_err(sql_err)?;
                if changed == 0 {
                    return Err("Ce type n'existe plus\u{a0}: il a été supprimé entre-temps.".into());
                }
            }
            None => t.id = Some(insert_type(&tx, &t).map_err(sql_err)?),
        }
        tx.commit().map_err(sql_err)?;
        Ok(t)
    }

    /// Supprime un type (les lots déjà générés gardent son nom dans l'historique).
    pub fn delete_type(&self, id: i64) -> Result<(), String> {
        self.conn().execute("DELETE FROM header_types WHERE id = ?1", [id]).map_err(sql_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enregistre_recherche_retelecharge_supprime() {
        let dir = std::env::temp_dir().join(format!("mmc_batch_db_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(dir.join("lots.sqlite")).unwrap();
        let csv = dir.join("export.csv");
        std::fs::write(&csv, "H\r\nMSISDN,237653282055,SouthWest\r\n").unwrap();

        let batch = NewBatch {
            source_name: "liste.xlsx",
            type_name: Some("AgentFloatRegion"),
            header: Some("H"),
            template: "MSISDN,‹numéro›,SouthWest",
            data_lines: 1,
            total_lines: 2,
        };
        let id = db.insert(&batch, &csv).unwrap();
        assert_eq!(db.info().unwrap().batch_count, 1);

        // Recherche d'un numéro dans le contenu des CSV.
        let found = db.list("237653282055").unwrap();
        assert_eq!((found.len(), found[0].size_bytes), (1, 34));
        assert_eq!(found[0].type_name.as_deref(), Some("AgentFloatRegion"));
        assert!(db.list("237699999999").unwrap().is_empty());

        // Premières lignes et ligne où se trouve le numéro cherché.
        let lines = db.lines(id, 5, "237653282055").unwrap();
        assert_eq!(lines.lines, ["H", "MSISDN,237653282055,SouthWest"]);
        assert_eq!(lines.found.map(|f| f.number), Some(2));
        assert_eq!(db.info().unwrap().number_count, 1);

        // Re-téléchargement identique, octet pour octet, sans écraser.
        let out = dir.join("sortie");
        let first = db.export(id, &out).unwrap();
        let second = db.export(id, &out).unwrap();
        assert_eq!(std::fs::read(&first).unwrap(), std::fs::read(&csv).unwrap());
        assert_ne!(first, second);

        db.delete(id).unwrap();
        assert_eq!(db.info().unwrap().batch_count, 0);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn types_d_en_tete_crees_modifies_supprimes() {
        use crate::header_types::{Source, TypeField};
        let dir = std::env::temp_dir().join(format!("mmc_batch_types_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("types.sqlite");
        std::fs::remove_file(&path).ok();
        let db = Db::open(path.clone()).unwrap();

        // Une base neuve reçoit l'exemple une seule fois.
        let names = |db: &Db| db.list_types().unwrap().into_iter().map(|t| t.name).collect::<Vec<_>>();
        assert_eq!(names(&db), ["AgentFloatRegion"]);

        let draft = HeaderType {
            name: "Aloys".into(),
            header: "Aloys,{{total}}".into(),
            fields: vec![
                TypeField { name: "nametype".into(), description: "le type du nom".into(), required: true, ..Default::default() },
                TypeField { name: "MSISDN".into(), source: Source::Numero, ..Default::default() },
            ],
            ..Default::default()
        };
        let saved = db.save_type(draft.clone()).unwrap();
        assert!(saved.id.is_some());
        assert_eq!(saved.fields[0].description, "le type du nom");
        // Un nom déjà pris est refusé, sauf pour le type lui-même.
        assert!(db.save_type(HeaderType { name: "aloys".into(), ..draft }).unwrap_err().contains("déjà"));
        let renamed = db.save_type(HeaderType { name: "Aloys 2".into(), ..saved.clone() }).unwrap();
        assert_eq!(renamed.id, saved.id);
        assert_eq!(db.list_types().unwrap().iter().find(|t| t.id == saved.id).unwrap(), &renamed);

        // Supprimer tous les types : l'exemple ne revient pas à la réouverture.
        for t in db.list_types().unwrap() {
            db.delete_type(t.id.unwrap()).unwrap();
        }
        drop(db);
        let db = Db::open(path).unwrap();
        assert!(db.list_types().unwrap().is_empty());
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migration_v1_v2_remplace_les_types_et_garde_les_lots() {
        let dir = std::env::temp_dir().join(format!("mmc_batch_migration_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v1.sqlite");
        std::fs::remove_file(&path).ok();
        {
            // Base en version 1 : un lot et un ancien type (colonnes).
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(
                "ALTER TABLE batches ADD COLUMN type_name TEXT;
                 INSERT INTO batches (source_name, file_name, file_path, data_lines, total_lines, template, content, type_name)
                 VALUES ('a.xlsx', 'a.csv', 'a.csv', 1, 1, 'MSISDN,‹numéro›', X'41', 'CRPLMT');
                 INSERT INTO header_types (name, spec) VALUES ('CRPLMT', '{\"columns\":[\"{{numero}}\"]}');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let db = Db::open(path.clone()).unwrap();
        let types = db.list_types().unwrap();
        assert_eq!(types.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["AgentFloatRegion"]);
        assert_eq!(types[0].fields.len(), 4);
        let batches = db.list("").unwrap();
        assert_eq!((batches.len(), batches[0].type_name.as_deref()), (1, Some("CRPLMT")));
        drop(db);
        // Rouvrir ne refait pas la marche.
        let db = Db::open(path).unwrap();
        assert_eq!(db.list_types().unwrap().len(), 1);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }
}
