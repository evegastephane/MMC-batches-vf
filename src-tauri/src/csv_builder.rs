//! Construction du CSV. L'aperçu, la ligne modèle et le fichier final passent
//! par la MÊME fonction `build_line`, ce qui garantit qu'ils sont identiques.

use crate::header_types::{check_value, Source};
use crate::source::{col_letter, is_number_alias, tag_key, Sheet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Fin de ligne imposée (format Windows).
const EOL: &str = "\r\n";

/// Un field du type d'en-tête et sa valeur pour ce lot. Chaque field écrit dans la ligne donne
/// une colonne : le numéro du fichier, la valeur saisie (la même sur chaque ligne), ou un texte
/// fixe / une formule où `{{montant}}` désigne un autre field, ex. `CRPLMT_{{numero}}@{{montant}}`.
/// Un field obligatoire resté vide, ou une valeur qui ne correspond pas à son type (`kind` :
/// nombre, date…), bloque la génération.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Param {
    pub name: String,
    pub source: Source,
    pub formula: String,
    pub in_line: bool,
    pub value: String,
    pub required: bool,
    pub kind: String,
}

impl Default for Param {
    fn default() -> Self {
        Self {
            name: String::new(),
            source: Source::Saisie,
            formula: String::new(),
            in_line: true,
            value: String::new(),
            required: false,
            kind: String::new(),
        }
    }
}

impl Param {
    /// Valeur écrite : espaces de bord retirés.
    fn value(&self) -> &str {
        self.value.trim()
    }

    fn is_missing(&self) -> bool {
        self.required && self.value().is_empty()
    }

    /// Ce qui ne va pas dans une valeur renseignée, selon le type du field.
    fn problem(&self) -> Option<&'static str> {
        match self.value() {
            "" => None,
            value => check_value(&self.kind, value),
        }
    }

    fn label(&self) -> &str {
        self.name.trim()
    }

    /// Repère montré tant qu'un field obligatoire est vide : `‹Region›` (jamais écrit dans un fichier).
    fn placeholder(&self) -> String {
        format!("‹{}›", self.label())
    }
}

/// Balise réservée de l'en-tête : le nombre de lignes de données.
pub fn is_total_alias(key: &str) -> bool {
    matches!(key, "total" | "nombre" | "lignes" | "nombredelignes")
}

/// Valeur d'un field saisi telle qu'elle s'affiche, et sa provenance : la valeur saisie,
/// marquée si elle ne correspond pas au type, ou le repère `‹Region›` tant qu'elle manque.
fn param_piece(params: &[Param], index: usize) -> Piece {
    let p = &params[index];
    let (text, kind) = if p.is_missing() {
        (p.placeholder(), Kind::Missing)
    } else {
        (p.value().to_string(), if p.problem().is_some() { Kind::Invalid } else { Kind::Value })
    };
    Piece::Param { index, text, kind }
}

/// Morceau d'une colonne : texte fixe, numéro du fichier ou valeur d'un field saisi.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Cell(usize),
    /// Valeur du field `index` (ou son repère `‹Region›` tant qu'il manque).
    Param { index: usize, text: String, kind: Kind },
}

/// Colonnes analysées une seule fois avant d'écrire les lignes.
#[derive(Debug, Clone)]
pub struct Template(Vec<Vec<Piece>>);

impl Template {
    /// Une colonne par field écrit dans la ligne ; au moins une doit contenir le numéro du fichier.
    pub fn parse(params: &[Param], sheet: &Sheet) -> Result<Self, String> {
        let mut columns = Vec::new();
        for (index, p) in params.iter().enumerate().filter(|(_, p)| p.in_line) {
            let pieces = match p.source {
                Source::Numero => number(sheet).map(|piece| vec![piece]),
                Source::Saisie => Ok(vec![param_piece(params, index)]),
                Source::Formule => parse_formula(&p.formula, params, sheet),
            };
            let pieces =
                pieces.map_err(|e| format!("Colonne {} ({})\u{a0}: {e}", col_letter(columns.len()), p.label()))?;
            columns.push(pieces);
        }
        if columns.is_empty() {
            return Err("Ce type n'a aucune colonne\u{a0}: écrivez au moins un field dans la ligne.".into());
        }
        if !columns.iter().flatten().any(|p| matches!(p, Piece::Cell(_))) {
            return Err("Aucune colonne ne contient le numéro du fichier\u{a0}: ajoutez un field «\u{a0}Numéro du fichier\u{a0}» \
                        ou {{numero}} dans une formule."
                .into());
        }
        Ok(Self(columns))
    }

    /// Fields saisis dont la valeur est écrite (directement ou par une formule), dans l'ordre du type.
    fn used(&self) -> BTreeSet<usize> {
        self.0
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Piece::Param { index, .. } => Some(*index),
                _ => None,
            })
            .collect()
    }
}

/// Le numéro de chaque ligne du fichier.
fn number(sheet: &Sheet) -> Result<Piece, String> {
    sheet.key.map(Piece::Cell).ok_or_else(|| "aucune colonne de numéros reconnue dans ce fichier.".to_string())
}

/// Texte fixe ou formule : `{{x}}` désigne d'abord un field du type, sinon le numéro (`numero`,
/// `parametreprincipal`, `msisdn`).
fn parse_formula(text: &str, params: &[Param], sheet: &Sheet) -> Result<Vec<Piece>, String> {
    split_tags(text)
        .into_iter()
        .map(|raw| match raw {
            Raw::Text(t) => Ok(Piece::Text(t.to_string())),
            Raw::Tag(name) => {
                let key = tag_key(name);
                match params.iter().position(|p| !key.is_empty() && tag_key(&p.name) == key) {
                    Some(index) => match params[index].source {
                        Source::Saisie => Ok(param_piece(params, index)),
                        Source::Numero => number(sheet),
                        Source::Formule => Err(format!(
                            "«\u{a0}{{{{{name}}}}}\u{a0}» est une formule\u{a0}: une formule ne peut pas en contenir une autre."
                        )),
                    },
                    None if is_number_alias(&key) => number(sheet),
                    None => Err(format!("«\u{a0}{{{{{name}}}}}\u{a0}» n'est pas un field de ce type.")),
                }
            }
        })
        .collect()
}

/// Morceau brut d'un texte, avant de savoir à quoi correspond chaque balise.
#[derive(Debug, PartialEq)]
enum Raw<'a> {
    Text(&'a str),
    Tag(&'a str),
}

/// Découpe un texte en texte fixe et balises. Un `{{` jamais fermé reste du texte.
fn split_tags(text: &str) -> Vec<Raw<'_>> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start + 2..].find("}}") else { break };
        if start > 0 {
            pieces.push(Raw::Text(&rest[..start]));
        }
        pieces.push(Raw::Tag(&rest[start + 2..start + 2 + len]));
        rest = &rest[start + 2 + len + 2..];
    }
    if !rest.is_empty() {
        pieces.push(Raw::Text(rest));
    }
    pieces
}

/// Noms des balises d'un texte, dans l'ordre.
pub fn tags_in(text: &str) -> impl Iterator<Item = &str> {
    split_tags(text).into_iter().filter_map(|raw| match raw {
        Raw::Tag(name) => Some(name),
        Raw::Text(_) => None,
    })
}

/// Provenance d'un morceau affiché dans l'aperçu.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Texte fixe, guillemets d'échappement compris.
    Text,
    /// Valeur du fichier source (le numéro…).
    Excel,
    /// Valeur d'un champ renseigné.
    Value,
    /// Valeur d'un champ renseigné qui ne correspond pas à son type (lettres dans un nombre…).
    Invalid,
    /// Champ obligatoire encore vide : son repère `‹Région›`.
    Missing,
}

/// Morceau d'une ligne tel qu'il sera écrit (guillemets compris), avec sa provenance.
#[derive(Debug, Serialize, PartialEq)]
pub struct Seg {
    pub text: String,
    pub kind: Kind,
    /// Field saisi d'où vient le morceau (index dans les fields reçus).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<usize>,
}

/// Ajoute un morceau, collé au précédent s'il a la même provenance.
fn push_seg(segs: &mut Vec<Seg>, text: &str, kind: Kind, param: Option<usize>) {
    if text.is_empty() {
        return;
    }
    match segs.last_mut() {
        Some(last) if last.kind == kind && last.param == param => last.text.push_str(text),
        _ => segs.push(Seg { text: text.to_string(), kind, param }),
    }
}

/// Une ligne du fichier, et la même découpée par colonne pour l'affichage :
/// les morceaux de chaque colonne, recollés avec des virgules, redonnent `text` à l'identique.
#[derive(Debug, Serialize)]
pub struct PreviewLine {
    pub text: String,
    pub fields: Vec<Vec<Seg>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    /// En-tête tel qu'il sera écrit (`None` s'il n'est pas inclus).
    pub header: Option<String>,
    /// Le même, champ CSV par champ CSV (une cellule chacun), en morceaux pour repérer les valeurs :
    /// recollés avec des virgules, ils redonnent `header`.
    pub header_parts: Option<Vec<Vec<Seg>>>,
    /// Premières lignes de données, exactement comme dans le fichier.
    pub lines: Vec<PreviewLine>,
    /// Ligne modèle avec `‹numéro›`, `‹Nom›`…, seulement tant qu'aucun numéro n'est chargé.
    pub sample: Option<PreviewLine>,
    pub data_lines: usize,
    /// Lignes du fichier, en-tête compris.
    pub total_lines: usize,
    /// Champs obligatoires encore vides (libellés) : la génération attend qu'ils soient renseignés.
    pub missing: Vec<String>,
    /// Valeurs qui ne correspondent pas au type de leur champ, ex. « Montant : nombre attendu, ex. 10000 ».
    pub invalid: Vec<String>,
}

/// Nombre de lignes écrites dans un fichier.
#[derive(Debug, Clone, Copy)]
pub struct WriteStats {
    pub data_lines: usize,
    pub total_lines: usize,
}

/// Un champ est entouré de guillemets s'il contient `,` `"` ou un saut de ligne.
fn needs_quotes(field: &str) -> bool {
    field.contains([',', '"', '\n', '\r'])
}

/// Ajoute un champ, entre guillemets si besoin (guillemets intérieurs doublés).
fn push_field(buf: &mut String, field: &str) {
    if needs_quotes(field) {
        buf.push('"');
        buf.push_str(&field.replace('"', "\"\""));
        buf.push('"');
    } else {
        buf.push_str(field);
    }
}

/// Valeur de chaque morceau d'une colonne pour une ligne du fichier, et sa provenance.
fn values<'a>(pieces: &'a [Piece], row: &'a [String]) -> impl Iterator<Item = (&'a str, Kind, Option<usize>)> {
    pieces.iter().map(move |piece| match piece {
        Piece::Text(text) => (text.as_str(), Kind::Text, None),
        Piece::Cell(col) => (row.get(*col).map_or("", String::as_str), Kind::Excel, None),
        Piece::Param { index, text, kind } => (text.as_str(), *kind, Some(*index)),
    })
}

/// Écrit dans `buf` (vidé au préalable) la ligne CSV d'une ligne du fichier, sans fin de ligne.
/// Exemple : `MSISDN,237653282055,CRPLMT_237653282055@10000`.
/// L'échappement CSV s'applique à chaque colonne une fois les valeurs insérées.
pub fn build_line(buf: &mut String, row: &[String], template: &Template) {
    buf.clear();
    let mut field = String::new();
    for (i, pieces) in template.0.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        field.clear();
        values(pieces, row).for_each(|(text, ..)| field.push_str(text));
        push_field(buf, &field);
    }
}

/// La ligne (via `build_line`) et ses morceaux, échappés de la même façon, pour l'aperçu.
fn preview_line(row: &[String], template: &Template) -> PreviewLine {
    let mut text = String::new();
    build_line(&mut text, row, template);
    let fields = template
        .0
        .iter()
        .map(|pieces| {
            let quoted = needs_quotes(&values(pieces, row).map(|(t, ..)| t).collect::<String>());
            let mut segs = Vec::new();
            if quoted {
                push_seg(&mut segs, "\"", Kind::Text, None);
            }
            for (t, kind, param) in values(pieces, row) {
                let t = if quoted { t.replace('"', "\"\"") } else { t.to_string() };
                push_seg(&mut segs, &t, kind, param);
            }
            if quoted {
                push_seg(&mut segs, "\"", Kind::Text, None);
            }
            segs
        })
        .collect();
    PreviewLine { text, fields }
}

/// Découpe une ligne CSV en champs bruts (guillemets conservés) : recollés avec des virgules,
/// ils redonnent la ligne.
fn split_raw(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    fields.push(&line[start..]);
    fields
}

/// L'en-tête n'est écrit que s'il est demandé et non vide. `{{total}}` (alias `{{nombre}}`,
/// `{{lignes}}`) y devient le nombre de lignes de données (`‹total›` sans fichier) ; une autre
/// balise est écrite telle quelle (un type n'en enregistre pas). Renvoie aussi, champ CSV par
/// champ CSV, les morceaux pour l'aperçu.
fn render_header(header: &str, include_header: bool, total: Option<usize>) -> Option<(String, Vec<Vec<Seg>>)> {
    if !include_header || header.trim().is_empty() {
        return None;
    }
    let total = total.map_or_else(|| "‹total›".to_string(), |n| n.to_string());
    let cells: Vec<Vec<Seg>> = split_raw(header)
        .into_iter()
        .map(|field| {
            let mut segs = Vec::new();
            for raw in split_tags(field) {
                match raw {
                    Raw::Text(t) => push_seg(&mut segs, t, Kind::Text, None),
                    Raw::Tag(name) if is_total_alias(&tag_key(name)) => push_seg(&mut segs, &total, Kind::Text, None),
                    Raw::Tag(name) => push_seg(&mut segs, &format!("{{{{{name}}}}}"), Kind::Text, None),
                }
            }
            segs
        })
        .collect();
    let text = cells
        .iter()
        .map(|segs| segs.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join(",");
    Some((text, cells))
}

/// En-tête tel qu'il sera écrit (`None` s'il n'est pas inclus ou vide).
pub fn header_line(header: &str, include_header: bool, total: usize) -> Option<String> {
    render_header(header, include_header, Some(total)).map(|(text, _)| text)
}

/// Noms des fields obligatoires encore vides dont la valeur est écrite.
pub fn missing_params(template: &Template, params: &[Param]) -> Vec<String> {
    template.used().into_iter().map(|i| &params[i]).filter(|p| p.is_missing()).map(|p| p.label().to_string()).collect()
}

/// Valeurs écrites qui ne correspondent pas au type de leur field : « montant : nombre attendu, ex. 10000 ».
pub fn invalid_params(template: &Template, params: &[Param]) -> Vec<String> {
    template
        .used()
        .into_iter()
        .map(|i| &params[i])
        .filter_map(|p| p.problem().map(|problem| format!("{}\u{a0}: {problem}", p.label())))
        .collect()
}

/// Message de refus quand des fields manquent ou sont mal remplis (rien n'est écrit).
pub fn issues_error(missing: &[String], invalid: &[String]) -> Option<String> {
    let mut parts = Vec::new();
    match missing {
        [] => {}
        [one] => parts.push(format!("Field obligatoire à renseigner\u{a0}: {one}.")),
        many => parts.push(format!("Fields obligatoires à renseigner\u{a0}: {}.", many.join(", "))),
    }
    match invalid {
        [] => {}
        [one] => parts.push(format!("Valeur à corriger\u{a0}: {one}.")),
        many => parts.push(format!("Valeurs à corriger\u{a0}: {}.", many.join("\u{a0}; "))),
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// En-tête + `limit` premières lignes + totaux. Sans fichier chargé, une ligne modèle montre
/// le format avec `‹numéro›`, `‹Region›`…
pub fn preview(
    sheet: Option<&Sheet>,
    header: &str,
    include_header: bool,
    params: &[Param],
    limit: usize,
) -> Result<PreviewResult, String> {
    let model;
    let (sheet, total) = match sheet {
        Some(sheet) => (sheet, Some(sheet.rows.len())),
        None => {
            model = Sheet::model();
            (&model, None)
        }
    };
    let template = Template::parse(params, sheet)?;
    let (header_text, header_parts) = render_header(header, include_header, total).unzip();
    Ok(PreviewResult {
        lines: sheet.rows.iter().take(limit).map(|row| preview_line(row, &template)).collect(),
        sample: sheet.rows.is_empty().then(|| preview_line(&sheet.placeholder_row(), &template)),
        data_lines: sheet.rows.len(),
        total_lines: sheet.rows.len() + usize::from(header_text.is_some()),
        missing: missing_params(&template, params),
        invalid: invalid_params(&template, params),
        header: header_text,
        header_parts,
    })
}

/// Refus avant toute écriture : field obligatoire vide ou valeur hors type.
pub fn check_params(template: &Template, params: &[Param]) -> Result<(), String> {
    issues_error(&missing_params(template, params), &invalid_params(template, params)).map_or(Ok(()), Err)
}

/// Chemin libre dans `folder` : `nom.csv`, sinon `nom (2).csv`, `nom (3).csv`…
/// (un fichier existant n'est jamais écrasé).
pub fn unique_path(folder: &Path, file_name: &str) -> Result<PathBuf, String> {
    if file_name.trim().is_empty() || file_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(format!("Nom de fichier invalide\u{a0}: «\u{a0}{file_name}\u{a0}»."));
    }
    let (stem, ext) = match file_name.rfind('.') {
        Some(i) if i > 0 => file_name.split_at(i),
        _ => (file_name, ""),
    };
    let first = folder.join(file_name);
    if !first.exists() {
        return Ok(first);
    }
    Ok((2..)
        .map(|n| folder.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("suite infinie"))
}

/// Écrit le fichier en flux (UTF-8, `\r\n`), ligne par ligne, via un `BufWriter`.
/// Refusé tant qu'un field obligatoire est vide ou qu'une valeur ne correspond pas à son type.
pub fn write_csv(
    sheet: &Sheet,
    header: &str,
    include_header: bool,
    params: &[Param],
    path: &Path,
) -> Result<WriteStats, String> {
    let template = Template::parse(params, sheet)?;
    if sheet.rows.is_empty() {
        return Err("Aucune ligne à exporter : importez d'abord un fichier.".into());
    }
    check_params(&template, params)?;
    let header = header_line(header, include_header, sheet.rows.len());
    let file = File::create(path).map_err(|e| create_error(path, &e))?;
    let io_err = |e: io::Error| format!("Erreur d'écriture dans «\u{a0}{}\u{a0}»\u{a0}: {e}", path.display());

    let mut out = BufWriter::new(file);
    if let Some(h) = &header {
        out.write_all(h.as_bytes()).map_err(io_err)?;
        out.write_all(EOL.as_bytes()).map_err(io_err)?;
    }
    // Un seul tampon réutilisé pour toutes les lignes.
    let mut line = String::with_capacity(128);
    for row in &sheet.rows {
        build_line(&mut line, row, &template);
        line.push_str(EOL);
        out.write_all(line.as_bytes()).map_err(io_err)?;
    }
    out.flush().map_err(io_err)?;

    Ok(WriteStats {
        data_lines: sheet.rows.len(),
        total_lines: sheet.rows.len() + usize::from(header.is_some()),
    })
}

/// Erreur de création lisible, avec le cas fréquent du fichier ouvert dans Excel.
pub fn create_error(path: &Path, e: &io::Error) -> String {
    let path = path.display();
    if e.kind() == io::ErrorKind::PermissionDenied {
        format!("Impossible d'écrire «\u{a0}{path}\u{a0}»\u{a0}: accès refusé. Le fichier est peut-être ouvert dans Excel\u{a0}; fermez-le puis réessayez.")
    } else {
        format!("Impossible de créer «\u{a0}{path}\u{a0}»\u{a0}: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Column;

    const NUM: &str = "237653282055";

    /// Field « Numéro du fichier ».
    fn number_field() -> Param {
        Param { name: "MSISDN".into(), source: Source::Numero, ..Default::default() }
    }

    /// Field « Texte fixe / formule ».
    fn text(t: &str) -> Param {
        Param { name: format!("f{}", t.len()), source: Source::Formule, formula: t.into(), ..Default::default() }
    }

    fn named(name: &str, formula: &str) -> Param {
        Param { name: name.into(), source: Source::Formule, formula: formula.into(), ..Default::default() }
    }

    /// Field « Saisie au lot ».
    fn input(name: &str, value: &str, required: bool) -> Param {
        Param { name: name.into(), value: value.into(), required, ..Default::default() }
    }

    fn off_line(mut p: Param) -> Param {
        p.in_line = false;
        p
    }

    /// Fichier d'une seule colonne `msisdn`, avec les numéros donnés.
    fn numbers(nums: &[&str]) -> Sheet {
        Sheet {
            columns: vec![Column { letter: "A".into(), name: Some("msisdn".into()) }],
            key: Some(0),
            rows: nums.iter().map(|n| vec![n.to_string()]).collect(),
            ..Default::default()
        }
    }

    fn line_with(sheet: &Sheet, params: &[Param]) -> String {
        let mut buf = String::new();
        build_line(&mut buf, &sheet.rows[0], &Template::parse(params, sheet).unwrap());
        buf
    }

    fn line(params: &[Param]) -> String {
        line_with(&numbers(&[NUM]), params)
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mmc_batch_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Recolle les morceaux d'une ligne d'aperçu.
    fn rejoin(line: &PreviewLine) -> String {
        let fields: Vec<String> = line.fields.iter().map(|f| f.iter().map(|s| s.text.as_str()).collect()).collect();
        fields.join(",")
    }

    #[test]
    fn numero_a_n_importe_quelle_colonne() {
        assert_eq!(line(&[number_field(), text("AgentFloatRegion"), text("SouthWest")]), "237653282055,AgentFloatRegion,SouthWest");
        assert_eq!(line(&[text("MSISDN"), number_field(), text("SouthWest")]), "MSISDN,237653282055,SouthWest");
        assert_eq!(line(&[text("AgentFloatRegion"), text("MSISDN"), number_field()]), "AgentFloatRegion,MSISDN,237653282055");
    }

    #[test]
    fn crplmt_par_formule_et_field_hors_ligne() {
        let params = [
            text("MSISDN"),
            number_field(),
            named("Complément", "CRPLMT_{{numero}}@{{montant}}"),
            off_line(input("montant", "10000", true)),
        ];
        assert_eq!(line(&params), "MSISDN,237653282055,CRPLMT_237653282055@10000");
        // Le numéro peut aussi être cité par le nom de son field, et la valeur par le sien, accents et casse ignorés.
        let params = [number_field(), named("C", "{{msisdn}}-{{ Montant }}"), off_line(input("montant", "5", false))];
        assert_eq!(line(&params), "237653282055,237653282055-5");
    }

    #[test]
    fn texte_fixe_et_saisie() {
        let params = [input("nametype", "Particulier", true), number_field(), text(""), text("a{{b")];
        assert_eq!(line(&params), "Particulier,237653282055,,a{{b");
    }

    #[test]
    fn formules_verifiees() {
        let sheet = numbers(&[NUM]);
        let err = |params: &[Param]| Template::parse(params, &sheet).unwrap_err();
        // Formule dans une formule.
        let e = err(&[number_field(), named("A", "x"), named("Complément", "{{A}}")]);
        assert!(e.starts_with("Colonne C (Complément)") && e.contains("ne peut pas en contenir"), "{e}");
        // Field inconnu.
        let e = err(&[number_field(), named("B", "{{Prenom}}")]);
        assert!(e.starts_with("Colonne B (B)") && e.contains("«\u{a0}{{Prenom}}\u{a0}» n'est pas un field"), "{e}");
        // Aucun numéro dans la ligne.
        assert!(err(&[text("A"), input("x", "", false)]).contains("numéro du fichier"));
        assert!(err(&[off_line(number_field())]).contains("aucune colonne"));
        // Fichier sans colonne de numéros.
        let sans = Sheet { columns: vec![Column { letter: "A".into(), name: None }], rows: vec![vec!["x".into()]], ..Default::default() };
        assert!(Template::parse(&[number_field()], &sans).unwrap_err().contains("aucune colonne de numéros"));
    }

    #[test]
    fn echappement_apres_insertion() {
        let params = [text("Sud, Ouest"), named("n", "n° {{numero}}, ok"), text("dit \"ok\""), input("d", "Init, \"abroad\"", true)];
        assert_eq!(line(&params), "\"Sud, Ouest\",\"n° 237653282055, ok\",\"dit \"\"ok\"\"\",\"Init, \"\"abroad\"\"\"");
    }

    #[test]
    fn morceaux_de_l_apercu_identiques_a_la_ligne() {
        let sheet = numbers(&[NUM]);
        let params = [named("A", "{{x}} / {{numero}}"), text("MSISDN"), off_line(input("x", "dit \"Jo\", fils", false))];
        let p = preview_line(&sheet.rows[0], &Template::parse(&params, &sheet).unwrap());
        assert_eq!(p.text, "\"dit \"\"Jo\"\", fils / 237653282055\",MSISDN");
        assert_eq!(rejoin(&p), p.text);
        assert_eq!(p.fields[0][1], Seg { text: "dit \"\"Jo\"\", fils".into(), kind: Kind::Value, param: Some(2) });
        assert_eq!(p.fields[0][3], Seg { text: NUM.into(), kind: Kind::Excel, param: None });
    }

    #[test]
    fn total_dans_l_en_tete() {
        let sheet = numbers(&[NUM, "237600000001", "237600000002"]);
        let p = preview(Some(&sheet), "Aloys,\"{{total}}\",{{ Nombre }},{{autre}}", true, &[number_field()], 5).unwrap();
        assert_eq!(p.header.as_deref(), Some("Aloys,\"3\",3,{{autre}}"));
        assert_eq!(header_line("{{total}}", true, 0).as_deref(), Some("0"));
        let model = preview(None, "Aloys,{{total}}", true, &[number_field()], 5).unwrap();
        assert_eq!(model.header.as_deref(), Some("Aloys,‹total›"));
    }

    #[test]
    fn en_tete_absent_si_decoche_ou_vide() {
        let sheet = numbers(&[NUM]);
        let params = [number_field()];
        assert_eq!(preview(Some(&sheet), "H", false, &params, 5).unwrap().header, None);
        assert_eq!(preview(Some(&sheet), "  ", true, &params, 5).unwrap().header, None);
        let p = preview(Some(&sheet), "H", true, &params, 5).unwrap();
        assert_eq!((p.header.as_deref(), p.total_lines), (Some("H"), 2));
    }

    #[test]
    fn modele_d_un_type_sans_fichier() {
        let params = [input("nametype", "", true), number_field(), named("C", "CRPLMT_{{numero}}@{{montant}}"), off_line(input("montant", "", true))];
        let p = preview(None, "Aloys,{{total}}", true, &params, 5).unwrap();
        let sample = p.sample.unwrap();
        assert_eq!(sample.text, "‹nametype›,‹numéro›,CRPLMT_‹numéro›@‹montant›");
        assert_eq!(rejoin(&sample), sample.text);
        assert_eq!((p.data_lines, p.total_lines), (0, 1));
        assert_eq!(p.missing, ["nametype", "montant"]);
    }

    #[test]
    fn generation_bloquee_si_obligatoire_vide() {
        let sheet = numbers(&[NUM]);
        // Un field facultatif vide donne une colonne vide ; un field hors ligne inutilisé ne manque pas.
        let params = [text("MSISDN"), number_field(), input("Region", "  ", true), input("note", "", false), off_line(input("inutile", "", true))];
        let p = preview(Some(&sheet), "H", true, &params, 5).unwrap();
        assert_eq!(p.missing, ["Region"]);
        assert_eq!(p.lines[0].text, "MSISDN,237653282055,‹Region›,");
        assert_eq!(p.lines[0].fields[2][0].kind, Kind::Missing);
        let path = temp_dir("manquant").join("manquant.csv");
        let err = write_csv(&sheet, "H", true, &params, &path).unwrap_err();
        assert_eq!(err, "Field obligatoire à renseigner\u{a0}: Region.");
        assert!(!path.exists());
    }

    #[test]
    fn generation_bloquee_si_valeur_hors_type() {
        let sheet = numbers(&[NUM]);
        let mut montant = off_line(input("montant", "10 000", false));
        montant.kind = "nombre".into();
        let params = |m: &Param| vec![number_field(), named("C", "CRPLMT_{{numero}}@{{montant}}"), m.clone()];
        let p = preview(Some(&sheet), "", true, &params(&montant), 5).unwrap();
        assert_eq!(p.invalid, ["montant\u{a0}: nombre attendu, ex. 10000"]);
        assert_eq!(p.lines[0].fields[1][3], Seg { text: "10 000".into(), kind: Kind::Invalid, param: Some(2) });
        let path = temp_dir("hors_type").join("hors_type.csv");
        let err = write_csv(&sheet, "", true, &params(&montant), &path).unwrap_err();
        assert!(err.starts_with("Valeur à corriger") && err.contains("montant"), "{err}");
        assert!(!path.exists());
        montant.value = "10000".into();
        assert!(preview(Some(&sheet), "", true, &params(&montant), 5).unwrap().invalid.is_empty());
    }

    #[test]
    fn apercu_identique_au_fichier() {
        let nums: Vec<String> = (0..8).map(|i| format!("23765000000{i}")).collect();
        let sheet = numbers(&nums.iter().map(String::as_str).collect::<Vec<_>>());
        let params = [
            input("nametype", "Nom, \"X\"", true),
            number_field(),
            named("C", "CRPLMT_{{numero}}@{{montant}}"),
            off_line(input("montant", "10000", true)),
        ];
        let path = temp_dir("apercu").join("apercu.csv");

        let res = write_csv(&sheet, "Aloys,{{total}}", true, &params, &path).unwrap();
        let prev = preview(Some(&sheet), "Aloys,{{total}}", true, &params, 5).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Fins de ligne \r\n partout, aucune \n isolée.
        assert_eq!(content.matches("\r\n").count(), 9);
        assert_eq!(content.matches('\n').count(), 9);
        let file_lines: Vec<&str> = content.split("\r\n").collect();
        assert_eq!(file_lines[0], prev.header.as_deref().unwrap());
        assert_eq!(file_lines[0], "Aloys,8");
        assert_eq!(file_lines[1], "\"Nom, \"\"X\"\"\",237650000000,CRPLMT_237650000000@10000");
        let shown: Vec<&str> = prev.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(&file_lines[1..6], shown.as_slice());
        assert!(prev.lines.iter().all(|l| rejoin(l) == l.text));
        assert_eq!((res.total_lines, res.data_lines), (9, 8));
        assert_eq!(prev.total_lines, res.total_lines);
    }

    #[test]
    fn fichier_existant_jamais_ecrase() {
        let dir = temp_dir("unique");
        let first = unique_path(&dir, "export.csv").unwrap();
        std::fs::write(&first, "x").unwrap();
        let second = unique_path(&dir, "export.csv").unwrap();
        assert_eq!(second.file_name().unwrap(), "export (2).csv");
        assert!(unique_path(&dir, "..\\autre.csv").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
