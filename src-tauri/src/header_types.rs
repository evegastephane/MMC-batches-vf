//! Types d'en-tête : chaque type décrit un format de lot, une liste de fields (dans l'ordre des
//! colonnes de chaque ligne), chacun avec sa description, et la 1re ligne du fichier.
//! Exemple : IdentifierType (texte fixe MSISDN), MSISDN (numéro du fichier), Region (saisie au lot).

use crate::csv_builder::{is_total_alias, tags_in, Param, Template};
use crate::source::{tag_key, Sheet};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeaderType {
    /// `None` tant que le type n'est pas enregistré.
    pub id: Option<i64>,
    pub name: String,
    pub description: String,
    /// 1re ligne du fichier, vide = pas d'en-tête : texte fixe et `{{total}}` (le nombre de lignes),
    /// jamais un field.
    pub header: String,
    /// Fields du type, dans l'ordre des colonnes du fichier : ce qui compose chaque ligne.
    pub fields: Vec<TypeField>,
}

/// D'où vient la valeur d'un field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Saisie au lot (la même valeur sur chaque ligne).
    #[default]
    Saisie,
    /// Le numéro de chaque ligne du fichier chargé.
    Numero,
    /// Texte fixe, ou formule mêlant d'autres fields : `CRPLMT_{{numero}}@{{montant}}`.
    Formule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TypeField {
    pub name: String,
    /// Pour aider la personne qui fera le lot : montrée en entier au moment de la saisie.
    pub description: String,
    pub source: Source,
    /// Texte fixe ou formule (source `Formule` seulement).
    pub formula: String,
    /// Écrit dans la ligne (une colonne) ; sinon le field ne sert qu'aux formules.
    pub in_line: bool,
    /// Facultatif par défaut ; seuls quelques fields le sont absolument.
    pub required: bool,
    /// Valeur proposée d'office à chaque lot (modifiable).
    pub default_value: String,
    /// Type de la valeur, parmi `KINDS` : elle est vérifiée avant d'écrire le fichier.
    pub kind: String,
    /// Type deviné d'après le nom du field (et sa valeur proposée), pas encore confirmé.
    pub guessed: bool,
}

impl Default for TypeField {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            source: Source::Saisie,
            formula: String::new(),
            in_line: true,
            required: false,
            default_value: String::new(),
            kind: String::new(),
            guessed: false,
        }
    }
}

/// Types de valeur d'un champ.
pub const KINDS: [&str; 5] = ["texte", "nombre", "date", "identifiant", "telephone"];

/// Mots d'un nom de champ : `extId` → `ext`, `id` ; `date_debut` → `date`, `debut` (accents retirés).
fn words(name: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut prev: Option<char> = None;
    for c in name.chars() {
        if !c.is_alphanumeric() {
            out.push(String::new());
            prev = None;
            continue;
        }
        // Nouveau mot à une majuscule après une minuscule (extId) ou entre lettres et chiffres (id2).
        let split = prev.is_some_and(|p| {
            (c.is_uppercase() && p.is_lowercase()) || (c.is_ascii_digit() != p.is_ascii_digit())
        });
        if split {
            out.push(String::new());
        }
        prev = Some(c);
        out.last_mut().expect("au moins un mot").push_str(&tag_key(&c.to_string()));
    }
    out.retain(|w| !w.is_empty());
    out
}

/// Type d'un field deviné d'après son nom, puis sa valeur proposée.
pub fn guess_kind(name: &str, default_value: &str) -> &'static str {
    let words = words(name);
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    // Débuts de mots : « IDEnti », « identifiant », « quantites »…
    let starts = |list: &[&str]| words.iter().any(|w| list.iter().any(|p| w.starts_with(p)));
    if has(&["tel", "phone", "mobile", "gsm", "portable"]) || starts(&["msisdn", "telephon"]) {
        "telephone"
    } else if has(&["jour", "day", "debut", "fin", "start", "end", "expire", "naissance"])
        || starts(&["date", "expiration", "echeance"])
    {
        "date"
    } else if has(&[
        "prix", "price", "somme", "solde", "balance", "nombre", "nb", "qte", "qty", "count", "valeur", "value", "taux",
        "rate", "frais", "fee", "fees", "bonus", "commission", "seuil", "limite", "limit", "plafond", "duree",
    ]) || starts(&["montant", "amount", "quantit", "pourcent", "percent"])
    {
        "nombre"
    } else if has(&["id", "extid", "code", "ref", "cle", "key", "uuid"]) || starts(&["ident", "matricul", "referen"]) {
        "identifiant"
    } else if check_value("date", default_value.trim()).is_none() && !default_value.trim().is_empty() {
        "date"
    } else if check_value("nombre", default_value.trim()).is_none() && !default_value.trim().is_empty() {
        "nombre"
    } else {
        "texte"
    }
}

fn is_date(value: &str) -> bool {
    let parts: Vec<&str> = value.split('/').collect();
    let [d, m, y] = parts.as_slice() else { return false };
    if d.len() != 2 || m.len() != 2 || y.len() != 4 || !value.chars().all(|c| c.is_ascii_digit() || c == '/') {
        return false;
    }
    let (Ok(d), Ok(m), Ok(y)) = (d.parse::<u32>(), m.parse::<u32>(), y.parse::<u32>()) else { return false };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// Ce qui ne va pas dans une valeur (non vide) pour un type de champ ; `None` si elle convient.
pub fn check_value(kind: &str, value: &str) -> Option<&'static str> {
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let ok = match kind {
        "nombre" => {
            let unsigned = value.strip_prefix('-').unwrap_or(value);
            match unsigned.split_once('.') {
                Some((int, dec)) => digits(int) && digits(dec),
                None => digits(unsigned),
            }
        }
        "date" => is_date(value),
        "identifiant" => value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "telephone" => {
            let n = value.strip_prefix('+').unwrap_or(value);
            digits(n) && (8..=15).contains(&n.len())
        }
        _ => true,
    };
    (!ok).then_some(match kind {
        "nombre" => "nombre attendu, ex. 10000",
        "date" => "date attendue au format JJ/MM/AAAA",
        "identifiant" => "lettres, chiffres, - _ ou . sans espace",
        _ => "numéro attendu, 8 à 15 chiffres",
    })
}

impl TypeField {
    /// Type de valeur absent ou inconnu : deviné d'après le nom, et marqué comme tel.
    fn fill_kind(&mut self) {
        if !KINDS.contains(&self.kind.as_str()) {
            self.kind = guess_kind(&self.name, &self.default_value).to_string();
            self.guessed = true;
        }
    }

    /// Le field et sa valeur proposée, tels que les reçoit la construction du fichier.
    pub fn param(&self) -> Param {
        Param {
            name: self.name.clone(),
            source: self.source,
            formula: self.formula.clone(),
            in_line: self.in_line,
            value: self.default_value.clone(),
            required: self.required,
            kind: self.kind.clone(),
        }
    }
}

impl HeaderType {
    /// Types de valeur manquants : devinés.
    pub fn fill_kinds(&mut self) {
        self.fields.iter_mut().for_each(TypeField::fill_kind);
    }

    /// Type prêt à enregistrer : textes nettoyés, type de valeur deviné au besoin, refus clair
    /// d'un type inutilisable. `taken` : noms des autres types.
    pub fn normalized(mut self, taken: &[String]) -> Result<Self, String> {
        self.name = self.name.trim().to_string();
        self.description = self.description.trim().to_string();
        if self.name.is_empty() {
            return Err("Donnez un nom au type.".into());
        }
        if taken.iter().any(|n| n.trim().to_lowercase() == self.name.to_lowercase()) {
            return Err(format!("Un type s'appelle déjà «\u{a0}{}\u{a0}»\u{a0}: choisissez un autre nom.", self.name));
        }
        let mut seen = HashSet::new();
        for (i, f) in self.fields.iter_mut().enumerate() {
            f.name = f.name.trim().to_string();
            f.description = f.description.trim().to_string();
            if f.name.is_empty() {
                return Err(format!("Le field n°\u{a0}{} n'a pas de nom.", i + 1));
            }
            if f.name.contains(['{', '}']) {
                return Err(format!("Nom de field «\u{a0}{}\u{a0}»\u{a0}: retirez les accolades.", f.name));
            }
            if !seen.insert(tag_key(&f.name)) {
                return Err(format!("Deux fields s'appellent «\u{a0}{}\u{a0}»\u{a0}: donnez-leur des noms différents.", f.name));
            }
            if f.source != Source::Formule {
                f.formula.clear();
            }
            f.fill_kind();
        }
        if !self.fields.iter().any(|f| f.in_line) {
            return Err("Ajoutez au moins une colonne\u{a0}: un field écrit dans la ligne.".into());
        }
        if let Some(tag) = tags_in(&self.header).find(|t| !is_total_alias(&tag_key(t))) {
            return Err(format!(
                "La ligne d'en-tête ne contient pas de field\u{a0}: retirez «\u{a0}{{{{{tag}}}}}\u{a0}» (seul le nombre de lignes y est calculé)."
            ));
        }
        for f in self.fields.iter().filter(|f| !f.in_line) {
            if f.source != Source::Saisie {
                return Err(format!(
                    "«\u{a0}{}\u{a0}» n'est pas écrit dans la ligne\u{a0}: seul un field saisi au lot peut l'être.",
                    f.name
                ));
            }
            let key = tag_key(&f.name);
            let used = self
                .fields
                .iter()
                .filter(|g| g.in_line && g.source == Source::Formule)
                .any(|g| tags_in(&g.formula).any(|t| tag_key(t) == key));
            if !used {
                return Err(format!(
                    "«\u{a0}{}\u{a0}» n'est pas écrit dans la ligne et aucune formule ne s'en sert\u{a0}: cochez «\u{a0}Écrit dans la ligne\u{a0}» ou utilisez-le dans une formule.",
                    f.name
                ));
            }
        }
        // Formules, numéro dans la ligne : la même règle qu'à la génération.
        let params: Vec<Param> = self.fields.iter().map(TypeField::param).collect();
        Template::parse(&params, &Sheet::model())?;
        Ok(self)
    }
}

fn field(name: &str, description: &str, source: Source, formula: &str) -> TypeField {
    TypeField { name: name.into(), description: description.into(), source, formula: formula.into(), ..Default::default() }
}

/// Type proposé à la création de la base, tiré des fichiers réels : à ajuster ou supprimer.
pub fn examples() -> Vec<HeaderType> {
    let mut region = field("Region", "Région de float de l'agent, ex. SouthWest, Littoral, abroad.", Source::Saisie, "");
    region.required = true;
    let mut t = HeaderType {
        name: "AgentFloatRegion".into(),
        description: "UpdateIndividualRatingParameter : région de float des agents".into(),
        header: "AgentFloatRegion,{{total}}".into(),
        fields: vec![
            field("IdentifierType", "Type d'identifiant de l'agent : toujours MSISDN.", Source::Formule, "MSISDN"),
            field("MSISDN", "Numéro de l'agent, pris dans le fichier chargé.", Source::Numero, ""),
            field("Parameter", "Paramètre mis à jour : toujours AgentFloatRegion.", Source::Formule, "AgentFloatRegion"),
            region,
        ],
        ..Default::default()
    };
    t.fill_kinds();
    vec![t]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(header: &str, fields: Vec<TypeField>) -> HeaderType {
        HeaderType { name: " Mon type ".into(), header: header.into(), fields, ..Default::default() }
    }

    fn numero() -> TypeField {
        field("MSISDN", "", Source::Numero, "")
    }

    fn saisie(name: &str) -> TypeField {
        field(name, "", Source::Saisie, "")
    }

    #[test]
    fn type_nettoye_et_devine() {
        let mut nt = saisie(" nametype ");
        nt.description = " le type du nom ".into();
        let mut montant = saisie("montant");
        montant.in_line = false;
        let complement = field("Complément", "", Source::Formule, "CRPLMT_{{numero}}@{{montant}}");
        let t = draft(" Aloys,{{total}} ", vec![nt, numero(), complement, montant]).normalized(&[]).unwrap();
        assert_eq!(t.name, "Mon type");
        assert_eq!((t.fields[0].name.as_str(), t.fields[0].description.as_str()), ("nametype", "le type du nom"));
        assert_eq!((t.fields[3].kind.as_str(), t.fields[3].guessed), ("nombre", true));
    }

    #[test]
    fn type_de_valeur_devine() {
        assert_eq!(guess_kind("montant", ""), "nombre");
        assert_eq!(guess_kind("extId", ""), "identifiant");
        assert_eq!(guess_kind("ref2", ""), "identifiant");
        assert_eq!(guess_kind("date_debut", ""), "date");
        assert_eq!(guess_kind("Téléphone du gérant", ""), "telephone");
        assert_eq!(guess_kind("region", "SouthWest"), "texte");
        assert_eq!(guess_kind("palier", "5000"), "nombre");
        assert_eq!(guess_kind("lancement", "01/10/2026"), "date");
        // Un mot qui contient « id » sans l'être ne fait pas un identifiant.
        assert_eq!(guess_kind("valide", ""), "texte");
        // Champs réels : « IDEnti » à remplir de 0000000, « couleur » à remplir de rouge.
        assert_eq!(guess_kind("IDEnti", "0000000"), "identifiant");
        assert_eq!(guess_kind("couleur", "rouge"), "texte");
        assert_eq!(check_value("identifiant", "0000000"), None);
    }

    #[test]
    fn valeurs_controlees_selon_le_type() {
        assert_eq!(check_value("nombre", "10000"), None);
        assert_eq!(check_value("nombre", "-12.5"), None);
        assert!(check_value("nombre", "10 000").is_some());
        assert!(check_value("nombre", "1e5").is_some());
        assert_eq!(check_value("date", "29/02/2024"), None);
        assert!(check_value("date", "29/02/2026").is_some());
        assert!(check_value("date", "2026-10-16").is_some());
        assert_eq!(check_value("identifiant", "ExtId_12340"), None);
        assert!(check_value("identifiant", "Ext 1").is_some());
        assert_eq!(check_value("telephone", "237653282055"), None);
        assert_eq!(check_value("telephone", "+237653282055"), None);
        assert!(check_value("telephone", "6532").is_some());
        assert_eq!(check_value("texte", "n'importe quoi, vraiment"), None);
    }

    #[test]
    fn type_inutilisable_refuse() {
        let err = |t: HeaderType| t.normalized(&[]).unwrap_err();
        assert!(draft("", vec![numero()]).normalized(&["mon TYPE".into()]).unwrap_err().contains("déjà"));
        assert!(HeaderType { fields: vec![numero()], ..Default::default() }.normalized(&[]).is_err());
        assert!(err(draft("", vec![])).contains("colonne"));
        // Un field dans l'en-tête.
        assert!(err(draft("Aloys,{{nametype}}", vec![numero(), saisie("nametype")])).contains("ne contient pas de field"));
        // Deux fields au même nom (casse et accents ignorés), sans nom, avec accolades.
        assert!(err(draft("", vec![numero(), saisie("Région"), saisie("region")])).contains("Deux fields"));
        assert!(err(draft("", vec![numero(), saisie(" ")])).contains("pas de nom"));
        assert!(err(draft("", vec![numero(), saisie("a{b")])).contains("accolades"));
        // Hors ligne inutilisé, ou hors ligne sans être saisi.
        let mut libre = saisie("libre");
        libre.in_line = false;
        assert!(err(draft("", vec![numero(), libre])).contains("aucune formule"));
        let mut num = numero();
        num.in_line = false;
        assert!(err(draft("", vec![num, saisie("x")])).contains("seul un field saisi"));
        // Formule dans une formule, field inconnu, aucune colonne de numéro.
        let a = field("A", "", Source::Formule, "x");
        let b = field("B", "", Source::Formule, "{{A}}");
        assert!(err(draft("", vec![numero(), a.clone(), b])).contains("ne peut pas en contenir"));
        assert!(err(draft("", vec![numero(), field("B", "", Source::Formule, "{{inconnu}}")])).contains("n'est pas un field"));
        assert!(err(draft("", vec![saisie("x"), a])).contains("numéro"));
    }

    #[test]
    fn exemple_valide() {
        let list = examples();
        assert_eq!(list.len(), 1);
        for t in list {
            let name = t.name.clone();
            assert_eq!(t.clone().normalized(&[]).unwrap(), t, "{name}");
        }
    }
}
