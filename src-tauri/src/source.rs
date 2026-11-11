//! Lecture du fichier source, Excel (au choix de la feuille) ou CSV, en un tableau de textes :
//! colonnes (lettre et éventuel nom d'en-tête), lignes nettoyées et dédoublonnées,
//! colonne des numéros repérée, ligne `HDR` de tête mise de côté pour l'en-tête du CSV.

use calamine::{open_workbook_auto, Data, ExcelDateTime, Reader};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

/// Texte montré à la place du numéro dans la ligne modèle.
pub const SAMPLE_NUMBER: &str = "‹numéro›";

/// En-tête de la colonne des numéros, quand le fichier en a un.
const KEY_NAME: &str = "msisdn";

/// Longueur maximale d'un en-tête utilisable comme nom de balise.
const MAX_NAME_LEN: usize = 40;

/// Longueur maximale de l'exemple de valeur montré pour chaque colonne.
const MAX_SAMPLE_LEN: usize = 24;

/// Premières lignes du fichier renvoyées pour l'aperçu de la source (cellules raccourcies).
const FIRST_ROWS: usize = 14;
const MAX_CELL_LEN: usize = 60;

/// Une colonne du tableau.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Column {
    /// Lettre de la colonne : celle d'Excel, ou A, B, C… dans l'ordre des champs d'un CSV.
    pub letter: String,
    /// En-tête de la 1re ligne, s'il est utilisable comme nom de balise.
    pub name: Option<String>,
}

impl Column {
    /// Nom à écrire dans la balise : l'en-tête, sinon la lettre.
    pub fn tag(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.letter)
    }
}

/// Fichier lu : colonnes et une ligne de cellules par ligne retenue.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub columns: Vec<Column>,
    /// Colonne des numéros (`{{numero}}`) : en-tête `msisdn`, sinon 1re colonne de numéros.
    pub key: Option<usize>,
    /// Cellules de chaque ligne retenue, alignées sur `columns`.
    pub rows: Vec<Vec<String>>,
    /// Lignes de données lues (avant dédoublonnage).
    pub total_read: usize,
    pub duplicates_removed: usize,
    /// Lignes ignorées faute de numéro.
    pub skipped: usize,
    /// La 1re ligne donne les noms des colonnes.
    pub header_row: bool,
    /// Ligne `HDR,…` trouvée en tête du fichier, son nombre de lignes remplacé par `{{total}}`.
    pub batch_header: Option<String>,
    /// Feuilles du classeur (vide pour un CSV) et feuille lue.
    pub sheets: Vec<String>,
    pub sheet_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub letter: String,
    pub name: Option<String>,
    /// Balise insérée par la pastille de la colonne (`numero` pour la colonne des numéros).
    pub tag: String,
    /// Valeur de la 1re ligne, pour reconnaître la colonne.
    pub sample: String,
}

/// Résumé renvoyé au frontend : les lignes restent côté Rust.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetSummary {
    pub sheets: Vec<String>,
    pub sheet: String,
    pub header_row: bool,
    pub columns: Vec<ColumnInfo>,
    pub has_key: bool,
    /// Toutes les balises valides pour ce fichier.
    pub tags: Vec<String>,
    /// Éléments qui reproduisent le fichier : texte fixe pour une colonne constante, balise sinon.
    pub suggested: Vec<String>,
    pub batch_header: Option<String>,
    /// Premières lignes retenues, telles que lues (aperçu de la source).
    pub first_rows: Vec<Vec<String>>,
    pub count: usize,
    pub total_read: usize,
    pub duplicates_removed: usize,
    pub skipped: usize,
}

impl Sheet {
    pub fn summary(&self) -> SheetSummary {
        let first = self.rows.first();
        let columns = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| ColumnInfo {
                letter: c.letter.clone(),
                name: c.name.clone(),
                tag: if Some(i) == self.key { "numero".into() } else { c.tag().into() },
                sample: first.map(|r| r[i].chars().take(MAX_SAMPLE_LEN).collect()).unwrap_or_default(),
            })
            .collect();
        let mut tags: Vec<String> = self
            .columns
            .iter()
            .flat_map(|c| c.name.iter().chain([&c.letter]).cloned())
            .collect();
        if self.key.is_some() {
            tags.extend(["numero", "parametreprincipal", KEY_NAME].map(String::from));
        }
        SheetSummary {
            sheets: self.sheets.clone(),
            sheet: self.sheet_name.clone(),
            header_row: self.header_row,
            columns,
            has_key: self.key.is_some(),
            tags,
            suggested: self.suggested(),
            batch_header: self.batch_header.clone(),
            first_rows: self
                .rows
                .iter()
                .take(FIRST_ROWS)
                .map(|r| r.iter().map(|c| c.chars().take(MAX_CELL_LEN).collect()).collect())
                .collect(),
            count: self.rows.len(),
            total_read: self.total_read,
            duplicates_removed: self.duplicates_removed,
            skipped: self.skipped,
        }
    }

    /// Éléments qui redonnent le fichier tel quel : `{{numero}}` pour les numéros, texte fixe
    /// pour une colonne identique sur toutes les lignes (`MSISDN`, `AgentFloatRegion`…), balise sinon.
    fn suggested(&self) -> Vec<String> {
        (0..self.columns.len())
            .map(|i| {
                if Some(i) == self.key {
                    return "{{numero}}".to_string();
                }
                let first = self.rows.first().map_or("", |r| r[i].as_str());
                let constant = self.rows.len() > 1 && self.rows.iter().all(|r| r[i] == first);
                if constant && !first.contains("{{") {
                    first.to_string()
                } else {
                    format!("{{{{{}}}}}", self.columns[i].tag())
                }
            })
            .collect()
    }

    /// Colonne désignée par une balise : nom d'en-tête (casse, accents, espaces et `_` ignorés),
    /// puis lettre (`{{B}}`), puis `numero`, `parametreprincipal` ou `msisdn` pour les numéros.
    /// (Les formules ne citent plus les colonnes du fichier : gardé pour les tests de lecture.)
    #[cfg(test)]
    pub fn column(&self, tag: &str) -> Option<usize> {
        let tag = tag_key(tag);
        if tag.is_empty() {
            return None;
        }
        self.columns
            .iter()
            .position(|c| c.name.as_deref().is_some_and(|n| tag_key(n) == tag))
            .or_else(|| self.columns.iter().position(|c| tag_key(&c.letter) == tag))
            .or_else(|| if is_number_alias(&tag) { self.key } else { None })
    }

    /// Une ligne de repères : `‹numéro›` pour les numéros, `‹Nom›` ou `‹B›` pour les autres colonnes.
    pub fn placeholder_row(&self) -> Vec<String> {
        self.columns
            .iter()
            .enumerate()
            .map(|(i, c)| if Some(i) == self.key { SAMPLE_NUMBER.to_string() } else { format!("‹{}›", c.tag()) })
            .collect()
    }

    /// Tableau vide de la ligne modèle (aucun fichier chargé) : la seule colonne des numéros,
    /// pour montrer le format avec des repères.
    pub fn model() -> Self {
        Sheet {
            columns: vec![Column { letter: String::new(), name: Some(KEY_NAME.into()) }],
            key: Some(0),
            ..Default::default()
        }
    }
}

/// Clé de comparaison d'un nom de balise ou de colonne : `Numéro`, ` parametre_principal `…
pub fn tag_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

pub fn is_number_alias(key: &str) -> bool {
    matches!(key, "numero" | "parametreprincipal" | KEY_NAME)
}

/// Lettre Excel d'une colonne : 0 → A, 25 → Z, 26 → AA…
pub fn col_letter(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).expect("lettres ASCII")
}

/// Lignes brutes d'un fichier, avant interprétation.
struct Table {
    rows: Vec<Vec<String>>,
    /// Lettre de la 1re colonne (classeur commençant en colonne B…).
    first_col: usize,
    /// Première ligne non vide telle qu'écrite dans un CSV (pour garder une ligne HDR à l'identique).
    first_line: Option<String>,
    /// Séparateur des champs d'un CSV.
    delimiter: char,
    /// Les colonnes vides en fin de ligne sont du remplissage (classeur), pas des champs (CSV).
    trim_trailing: bool,
}

/// Lit un classeur (feuille `sheet`, sinon la 1re) ou un CSV, puis l'interprète.
/// `header_row` force la lecture de la 1re ligne comme noms de colonnes (ou non) ; `None` = détection.
pub fn read_file(path: &str, sheet: Option<&str>, header_row: Option<bool>) -> Result<Sheet, String> {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let (table, sheets, sheet_name) = if matches!(ext.as_str(), "csv" | "txt") {
        let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        (read_csv(path)?, Vec::new(), name)
    } else {
        read_workbook(path, sheet)?
    };
    let mut data = interpret(table, header_row);
    data.sheets = sheets;
    data.sheet_name = sheet_name;
    Ok(data)
}

fn read_workbook(path: &str, sheet: Option<&str>) -> Result<(Table, Vec<String>, String), String> {
    let mut workbook = open_workbook_auto(path).map_err(|e| format!("Impossible de lire le fichier Excel\u{a0}: {e}"))?;
    let sheets = workbook.sheet_names();
    let name = match sheet {
        Some(s) if sheets.iter().any(|n| n == s) => s.to_string(),
        _ => sheets.first().cloned().ok_or("Le fichier Excel ne contient aucune feuille.")?,
    };
    let range = workbook
        .worksheet_range(&name)
        .map_err(|e| format!("Impossible de lire la feuille «\u{a0}{name}\u{a0}»\u{a0}: {e}"))?;
    let table = Table {
        rows: range.rows().map(|r| r.iter().map(cell_to_text).collect()).collect(),
        first_col: range.start().map_or(0, |(_, c)| c as usize),
        first_line: None,
        delimiter: ',',
        trim_trailing: true,
    };
    Ok((table, sheets, name))
}

fn read_csv(path: &str) -> Result<Table, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Impossible de lire le fichier\u{a0}: {e}"))?;
    let text = decode(bytes);
    let delimiter = detect_delimiter(&text);
    Ok(Table {
        rows: parse_csv(&text, delimiter)
            .into_iter()
            .map(|r| r.into_iter().map(|c| c.trim().to_string()).collect())
            .collect(),
        first_col: 0,
        first_line: text.lines().find(|l| !l.trim().is_empty()).map(str::to_owned),
        delimiter,
        trim_trailing: false,
    })
}

/// UTF-8 (marque BOM retirée), sinon Latin-1 : un fichier enregistré par Excel « CSV (séparateur : point-virgule) ».
fn decode(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(s) => s.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(s),
        Err(e) => e.into_bytes().into_iter().map(char::from).collect(),
    }
}

/// Virgule, point-virgule ou tabulation : le plus fréquent hors guillemets sur les premières lignes.
fn detect_delimiter(text: &str) -> char {
    let sample: String = text.lines().take(10).collect::<Vec<_>>().join("\n");
    let mut counts = [(',', 0), (';', 0), ('\t', 0)];
    let mut quoted = false;
    for c in sample.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            if let Some(entry) = counts.iter_mut().find(|(d, _)| *d == c) {
                entry.1 += 1;
            }
        }
    }
    counts.iter().max_by_key(|(_, n)| *n).filter(|(_, n)| *n > 0).map_or(',', |(d, _)| *d)
}

/// Découpe un texte CSV : champs entre guillemets (`""` = guillemet), sauts de ligne `\n` ou `\r\n`.
fn parse_csv(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut at_start = true; // début de champ : un guillemet ouvre un champ entre guillemets
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' && chars.peek() == Some(&'"') {
                field.push('"');
                chars.next();
            } else if c == '"' {
                quoted = false;
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => quoted = true,
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                at_start = true;
                continue;
            }
            c if c == delimiter => {
                row.push(std::mem::take(&mut field));
                at_start = true;
                continue;
            }
            c => field.push(c),
        }
        at_start = false;
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Un nombre (avec ou sans espaces) : une ligne qui en contient n'est pas une ligne d'en-têtes.
fn is_numeric(cell: &str) -> bool {
    let s: String = cell.chars().filter(|c| !c.is_whitespace()).collect();
    s.chars().any(|c| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+'))
}

/// Un numéro de téléphone ou un identifiant : au moins 6 chiffres, `+` initial permis.
fn is_number_id(cell: &str) -> bool {
    let s: String = cell.chars().filter(|c| !c.is_whitespace()).collect();
    let digits = s.strip_prefix('+').unwrap_or(&s);
    digits.len() >= 6 && digits.chars().all(|c| c.is_ascii_digit())
}

/// Un en-tête sert de nom de balise s'il est court et sans caractère gênant.
fn usable_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME_LEN
        && !name.contains(['{', '}', ',', '"', '\n', '\r'])
}

/// Écrit des champs en ligne CSV (pour une ligne HDR lue dans un classeur).
fn join_fields(cells: &[String]) -> String {
    let end = cells.iter().rposition(|c| !c.is_empty()).map_or(0, |i| i + 1);
    cells[..end]
        .iter()
        .map(|c| if c.contains([',', '"']) { format!("\"{}\"", c.replace('"', "\"\"")) } else { c.clone() })
        .collect::<Vec<_>>()
        .join(",")
}

/// Dans une ligne HDR, remplace par `{{total}}` le champ égal au nombre de lignes du fichier
/// (s'il n'y en a qu'un) : le total suit alors les lignes réellement écrites.
fn with_total(line: &str, delimiter: char, counts: &[usize]) -> String {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == delimiter && !quoted {
            fields.push(&line[start..i]);
            start = i + c.len_utf8();
        }
    }
    fields.push(&line[start..]);
    for n in counts {
        let n = n.to_string();
        let unquote = |f: &str| f.trim().trim_matches('"').trim().to_string();
        let hits: Vec<usize> = (0..fields.len()).filter(|&i| unquote(fields[i]) == n).collect();
        if let &[i] = hits.as_slice() {
            let field = if fields[i].trim().starts_with('"') { "\"{{total}}\"" } else { "{{total}}" };
            let mut out: Vec<&str> = fields.clone();
            out[i] = field;
            return out.join(&delimiter.to_string());
        }
    }
    line.to_string()
}

/// Du tableau brut au fichier lu : cellules « ligne CSV » découpées, ligne HDR mise de côté,
/// en-têtes, colonne des numéros, lignes sans numéro ignorées, doublons retirés.
fn interpret(table: Table, header_row: Option<bool>) -> Sheet {
    let Table { mut rows, mut first_col, mut first_line, mut delimiter, mut trim_trailing } = table;
    rows.retain(|r| r.iter().any(|c| !c.is_empty()));

    // Classeur d'une seule colonne dont les cellules sont des lignes CSV : « ID,40432486,EleCashback, ».
    let single = rows.iter().all(|r| r.iter().skip(1).all(String::is_empty));
    let with_commas = rows.iter().filter(|r| r.first().is_some_and(|c| c.contains(','))).count();
    if single && with_commas * 2 > rows.len() {
        first_line = rows.first().map(|r| r[0].clone());
        rows = rows
            .iter()
            .map(|r| parse_csv(&r[0], ',').into_iter().next().unwrap_or_default().into_iter().map(|c| c.trim().to_string()).collect())
            .collect();
        (first_col, delimiter, trim_trailing) = (0, ',', false);
    }

    // Ligne de tête du lot : HDR,UpdateIndividualRatingParameter,…
    let hdr = rows.first().and_then(|r| r.first()).is_some_and(|c| c.trim().eq_ignore_ascii_case("HDR"));
    let batch_line = hdr.then(|| {
        let row = rows.remove(0);
        first_line.filter(|l| l.trim_start().get(..3).is_some_and(|p| p.eq_ignore_ascii_case("HDR"))).unwrap_or_else(|| join_fields(&row))
    });

    // Largeur : champs d'un CSV (une virgule finale compte), cellules remplies d'un classeur.
    let width = rows
        .iter()
        .map(|r| if trim_trailing { r.iter().rposition(|c| !c.is_empty()).map_or(0, |i| i + 1) } else { r.len() })
        .max()
        .unwrap_or(0);
    for r in &mut rows {
        r.resize(width, String::new());
    }

    let header_row = header_row.unwrap_or_else(|| {
        rows.len() >= 2 && rows[0].iter().any(|c| !c.is_empty()) && !rows[0].iter().any(|c| is_numeric(c))
    });
    let mut seen = HashSet::new();
    let columns: Vec<Column> = (0..width)
        .map(|i| Column {
            letter: col_letter(first_col + i),
            name: rows
                .first()
                .filter(|_| header_row)
                .map(|r| r[i].trim().to_string())
                .filter(|n| usable_name(n) && seen.insert(tag_key(n))),
        })
        .collect();
    if header_row && !rows.is_empty() {
        rows.remove(0);
    }

    let key = columns
        .iter()
        .position(|c| c.name.as_deref().is_some_and(|n| tag_key(n) == KEY_NAME))
        .or_else(|| {
            (0..width).find(|&i| {
                let mut values = rows.iter().map(|r| r[i].as_str()).filter(|v| !v.is_empty()).peekable();
                values.peek().is_some() && values.all(is_number_id)
            })
        });
    let lines_in_file = rows.len();
    let mut skipped = 0;
    if let Some(k) = key {
        // Numéros sans aucun espace (y compris insécables) ; une ligne sans numéro est ignorée.
        for r in &mut rows {
            r[k].retain(|c| !c.is_whitespace());
        }
        rows.retain(|r| !r[k].is_empty());
        skipped = lines_in_file - rows.len();
    }
    let total_read = rows.len();

    // Doublons : lignes identiques, la première est gardée.
    let keep: Vec<bool> = {
        let mut seen = HashSet::new();
        rows.iter().map(|r| seen.insert(r)).collect()
    };
    let mut flags = keep.into_iter();
    rows.retain(|_| flags.next().unwrap_or(true));

    let batch_header = batch_line.map(|line| with_total(&line, delimiter, &[lines_in_file, total_read, rows.len()]));
    Sheet {
        duplicates_removed: total_read - rows.len(),
        columns,
        key,
        rows,
        total_read,
        skipped,
        header_row,
        batch_header,
        ..Default::default()
    }
}

/// Texte d'une cellule tel qu'Excel l'affiche : espaces de bord retirés, nombres jamais
/// en notation scientifique ni avec `.0`, dates au format JJ/MM/AAAA.
pub fn cell_to_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::Int(n) => n.to_string(),
        Data::Float(f) => float_text(*f),
        Data::String(s) => s.trim().to_string(),
        Data::Bool(b) => (if *b { "TRUE" } else { "FALSE" }).to_string(),
        Data::DateTime(d) if d.is_datetime() => date_text(d),
        other => other.to_string().trim().to_string(),
    }
}

/// Un numéro saisi comme nombre arrive en flottant : on l'écrit en entier (exact jusqu'à 2^53),
/// sinon avec 15 chiffres significatifs au plus, comme Excel (12.35 et non 12.349999999999999).
fn float_text(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 9e15 {
        return (f as i64).to_string();
    }
    let int_digits = if f == 0.0 { 1 } else { f.abs().log10().floor() as i32 + 1 };
    let s = format!("{:.*}", (15 - int_digits).clamp(0, 15) as usize, f);
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// `25/12/2024`, `25/12/2024 08:30`, ou `08:30` pour une heure seule.
fn date_text(d: &ExcelDateTime) -> String {
    let (y, mo, day, h, mi, s, _) = d.to_ymd_hms_milli();
    let time = match s {
        0 => format!("{h:02}:{mi:02}"),
        _ => format!("{h:02}:{mi:02}:{s:02}"),
    };
    match (d.as_f64() < 1.0, (h, mi, s) == (0, 0, 0)) {
        (true, _) => time,
        (false, true) => format!("{day:02}/{mo:02}/{y}"),
        (false, false) => format!("{day:02}/{mo:02}/{y} {time}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::ExcelDateTimeType;

    fn date(serial: f64) -> Data {
        Data::DateTime(ExcelDateTime::new(serial, ExcelDateTimeType::DateTime, false))
    }

    /// Classeur : lignes de cellules texte (`""` = cellule vide).
    fn workbook(rows: &[&[&str]]) -> Table {
        Table {
            rows: rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect(),
            first_col: 0,
            first_line: None,
            delimiter: ',',
            trim_trailing: true,
        }
    }

    fn csv(text: &str) -> Table {
        let delimiter = detect_delimiter(text);
        Table {
            rows: parse_csv(text, delimiter).into_iter().map(|r| r.into_iter().map(|c| c.trim().to_string()).collect()).collect(),
            first_col: 0,
            first_line: text.lines().find(|l| !l.trim().is_empty()).map(str::to_owned),
            delimiter,
            trim_trailing: false,
        }
    }

    fn letters(s: &Sheet) -> Vec<&str> {
        s.columns.iter().map(|c| c.letter.as_str()).collect()
    }

    fn names(s: &Sheet) -> Vec<Option<&str>> {
        s.columns.iter().map(|c| c.name.as_deref()).collect()
    }

    #[test]
    fn cellules_ecrites_comme_dans_excel() {
        assert_eq!(cell_to_text(&Data::Float(237671598153.0)), "237671598153");
        assert_eq!(cell_to_text(&Data::Int(237653282055)), "237653282055");
        assert_eq!(cell_to_text(&Data::String("  Jean  Dupont ".into())), "Jean  Dupont");
        assert_eq!(cell_to_text(&Data::Float(12.349999999999999)), "12.35");
        assert_eq!(cell_to_text(&Data::Float(0.1 + 0.2)), "0.3");
        assert_eq!(cell_to_text(&Data::Bool(true)), "TRUE");
        assert_eq!(cell_to_text(&date(45651.0)), "25/12/2024");
        assert_eq!(cell_to_text(&date(45651.5)), "25/12/2024 12:00");
        assert_eq!(cell_to_text(&date(0.25)), "06:00");
        assert_eq!(cell_to_text(&Data::Empty), "");
    }

    #[test]
    fn lettres_de_colonnes() {
        assert_eq!([0, 1, 25, 26, 27, 51, 52, 701, 702].map(col_letter), ["A", "B", "Z", "AA", "AB", "AZ", "BA", "ZZ", "AAA"]);
    }

    #[test]
    fn colonne_msisdn_nettoyee_dedoublonnee() {
        let s = interpret(
            workbook(&[&["Nom", " MSISDN "], &["A", "237671598153"], &["B", ""], &["C", "237 653\u{a0}282 055"], &["A", "237671598153"]]),
            None,
        );
        assert!(s.header_row);
        assert_eq!(names(&s), [Some("Nom"), Some("MSISDN")]);
        assert_eq!(s.key, Some(1));
        assert_eq!(s.rows, [["A", "237671598153"], ["C", "237653282055"]]);
        assert_eq!((s.total_read, s.duplicates_removed, s.skipped), (3, 1, 1));
    }

    #[test]
    fn meme_numero_avec_d_autres_valeurs_garde_les_deux_lignes() {
        let s = interpret(workbook(&[&["msisdn", "zone"], &["237600000001", "West"], &["237600000001", "North"]]), None);
        assert_eq!((s.rows.len(), s.duplicates_removed), (2, 0));
    }

    #[test]
    fn cellules_lignes_csv_decoupees_sans_en_tete() {
        // Non_Eligible_Profile_To_Remove : une ligne CSV par cellule, champ final vide.
        let s = interpret(workbook(&[&["ID,40432486,EleCashback,"], &["ID,49240767,EleCashback,"], &["ID,40432486,EleCashback,"]]), None);
        assert!(!s.header_row);
        assert_eq!(letters(&s), ["A", "B", "C", "D"]);
        assert_eq!(s.key, Some(1));
        assert_eq!(s.rows[0], ["ID", "40432486", "EleCashback", ""]);
        assert_eq!((s.rows.len(), s.duplicates_removed), (2, 1));
        assert_eq!(s.suggested(), ["ID", "{{numero}}", "EleCashback", ""]);
    }

    #[test]
    fn csv_avec_ligne_hdr_reprise_et_total() {
        // to_add Abroad.csv : la ligne HDR donne le nombre de lignes (ici 3).
        let text = "HDR,UpdateIndividualRatingParameter,Init AgentFloatRegion abroad,ExtId12340,3, AgentFloatRegion init,1\r\n\
                    MSISDN,237676358432,AgentFloatRegion,abroad\r\n\
                    MSISDN,237677602246,AgentFloatRegion,abroad\r\n\
                    MSISDN,237653093670,AgentFloatRegion,abroad";
        let s = interpret(csv(text), None);
        assert_eq!(
            s.batch_header.as_deref(),
            Some("HDR,UpdateIndividualRatingParameter,Init AgentFloatRegion abroad,ExtId12340,{{total}}, AgentFloatRegion init,1")
        );
        assert!(!s.header_row);
        assert_eq!(s.rows.len(), 3);
        assert_eq!(s.suggested(), ["MSISDN", "{{numero}}", "AgentFloatRegion", "abroad"]);
    }

    #[test]
    fn hdr_entre_guillemets_et_valeurs_variables() {
        let text = "HDR,\"UpdateIndividualRatingParameter\",\"Init\",\"ExtId12340\",\"2\",\" init\",\"1\"\n\
                    MSISDN,237652395590,AgentFloatRegion,LittoralII\n\
                    MSISDN,237672608021,AgentFloatRegion,FarNorth\n";
        let s = interpret(csv(text), None);
        assert_eq!(s.batch_header.as_deref(), Some("HDR,\"UpdateIndividualRatingParameter\",\"Init\",\"ExtId12340\",\"{{total}}\",\" init\",\"1\""));
        assert_eq!(s.suggested(), ["MSISDN", "{{numero}}", "AgentFloatRegion", "{{D}}"]);
    }

    #[test]
    fn classeur_a_en_tetes_multiples() {
        // Pos_interbilling : accountholderid, msisdn, zone, profile.
        let s = interpret(
            workbook(&[
                &["accountholderid", "msisdn", "zone", "profile"],
                &["49274106", "237674497307", "South West", "MTNC Merchant Account Profile"],
                &["17164149", "237674931273", "West", "MTNC Agent Promo Profile 1.5"],
            ]),
            None,
        );
        assert_eq!(s.key, Some(1));
        assert_eq!(s.suggested(), ["{{accountholderid}}", "{{numero}}", "{{zone}}", "{{profile}}"]);
        assert_eq!(s.column("Zone"), Some(2));
        assert_eq!(s.column("c"), Some(2));
        assert_eq!(s.column("numéro"), Some(1));
    }

    #[test]
    fn premiere_ligne_exemple_non_utilisable_comme_nom() {
        // NEW FP LOGISTICS EMPLOYES : la ligne 1 est un exemple, les colonnes se prennent par leur lettre.
        let s = interpret(
            workbook(&[&["MSISDN,\"237653282055\",\"CRPLMT_237653282055@100000\"", "", ""], &["237653872683", "100000", ""], &["237653577509", "50000", ""]]),
            None,
        );
        assert!(s.header_row);
        assert_eq!(names(&s), [None, None]);
        assert_eq!(s.key, Some(0));
        assert_eq!(s.column("B"), Some(1));
        assert_eq!(s.rows, [["237653872683", "100000"], ["237653577509", "50000"]]);
    }

    #[test]
    fn choix_force_de_la_ligne_d_en_tetes() {
        let t = || workbook(&[&["Awa", "Douala"], &["Paul", "Buea"]]);
        assert_eq!(interpret(t(), None).rows.len(), 1);
        let s = interpret(t(), Some(false));
        assert_eq!((s.rows.len(), names(&s)), (2, vec![None, None]));
    }

    #[test]
    fn csv_point_virgule_et_guillemets() {
        let s = interpret(csv("msisdn;Nom\n237600000001;\"Dupont; \"\"Jo\"\"\"\n"), None);
        assert_eq!(s.rows, [["237600000001", "Dupont; \"Jo\""]]);
    }

    #[test]
    fn sans_colonne_de_numeros() {
        let s = interpret(workbook(&[&["Nom", "Ville"], &["Awa", "Douala"], &["Paul", "Buea"]]), None);
        assert_eq!(s.key, None);
        assert_eq!(s.column("numero"), None);
        assert_eq!(s.placeholder_row(), ["‹Nom›", "‹Ville›"]);
    }

    #[test]
    fn colonnes_trouvees_par_balise() {
        let s = Sheet {
            columns: vec![
                Column { letter: "A".into(), name: Some("Nom".into()) },
                Column { letter: "B".into(), name: Some("MSISDN".into()) },
                Column { letter: "C".into(), name: Some("Prénom usuel".into()) },
            ],
            key: Some(1),
            ..Default::default()
        };
        assert_eq!(s.column("nom"), Some(0));
        assert_eq!(s.column(" prenom_usuel "), Some(2));
        assert_eq!(s.column("Numéro"), Some(1));
        assert_eq!(s.column("parametre principal"), Some(1));
        assert_eq!(s.column("a"), Some(0));
        assert_eq!(s.column("Ville"), None);
        assert_eq!(s.placeholder_row(), ["‹Nom›", "‹numéro›", "‹Prénom usuel›"]);
    }
}
