// Aides d'affichage des lignes CSV (aperçu, types d'en-tête et historique).
// Elles ne construisent jamais de ligne : le texte vient toujours de Rust.

// Découpe une ligne en champs bruts (guillemets conservés), uniquement pour colorer
// chaque champ : les recoller avec des virgules redonne la ligne à l'identique.
export function splitRaw(line) {
  const fields = [];
  let start = 0;
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    if (line[i] === '"') quoted = !quoted;
    else if (line[i] === "," && !quoted) {
      fields.push(line.slice(start, i));
      start = i + 1;
    }
  }
  fields.push(line.slice(start));
  return fields;
}

// Balises des formules d'un type : {{numero}} (alias {{parametreprincipal}}, {{msisdn}}) pour le numéro
// du fichier, {{montant}} pour un autre field ; {{total}} dans l'en-tête pour le nombre de lignes.
// Rust les vérifie et les remplace ; ici, seulement pour l'affichage et l'édition.
export const NUMBER_TAG = "{{numero}}";
export const tagsIn = (text) => [...text.matchAll(/\{\{(.*?)\}\}/gs)].map((m) => m[1]);
// Même comparaison que Rust : casse, accents, espaces et `_` ignorés.
export const tagKey = (s) => s.normalize("NFD").replace(/[\u0300-\u036f]/g, "").replace(/[\s_]/g, "").toLowerCase();
export const isNumberTag = (tag) => ["numero", "parametreprincipal", "msisdn"].includes(tagKey(tag));
export const isTotalTag = (tag) => ["total", "nombre", "lignes", "nombredelignes"].includes(tagKey(tag));

// D'où vient la valeur d'un field (même liste que Rust, `Source`).
export const SOURCES = [
  { id: "saisie", label: "Saisie au lot" },
  { id: "numero", label: "Numéro du fichier" },
  { id: "formule", label: "Texte fixe / formule" },
];
export const sourceLabel = (id) => SOURCES.find((s) => s.id === id)?.label ?? "Saisie au lot";

// Tableau collé depuis Excel ou Word : une ligne par field, « Field ⇥ Description ⇥ Obligatoire ».
// Obligatoire : O, Oui, Yes, M, Mandatory, X… ; facultatif : N, Non, No, Optional, vide.
// Une 1re ligne d'intitulés (« Field », « Champ », « Nom »…) est ignorée.
export function parseFieldTable(text) {
  const yes = (v) => /^(o|oui|y|yes|m|mandatory|obligatoire|x|true|1)$/i.test(v.trim());
  const rows = text
    .split(/\r?\n/)
    .filter((l) => l.trim())
    .map((l) => l.split("\t").map((c) => c.trim()));
  if (rows.length && /^(field|champ|nom|name|param[eè]tre)s?$/i.test(rows[0][0] ?? "")) rows.shift();
  return rows
    .filter((r) => r[0])
    .map(([name, description = "", required = ""]) => ({ name: name.replace(/[{}]/g, ""), description, required: yes(required) }));
}

// Types de valeur d'un champ (Rust contrôle la valeur avant d'écrire le fichier).
export const KINDS = [
  { id: "texte", label: "Texte", hint: "tout est accepté" },
  { id: "nombre", label: "Nombre", hint: "chiffres, ex. 10000" },
  { id: "date", label: "Date", hint: "JJ/MM/AAAA" },
  { id: "identifiant", label: "Identifiant / code", hint: "lettres, chiffres, - _ ." },
  { id: "telephone", label: "Téléphone", hint: "8 à 15 chiffres" },
];
export const kindLabel = (id) => KINDS.find((k) => k.id === id)?.label ?? "Texte";

// Mots d'un nom : « extId » → ext, id ; « date_debut » → date, debut.
const wordsOf = (name) =>
  name
    .replace(/([a-z\d])([A-Z])/g, "$1 $2")
    .replace(/(\d)(\D)|(\D)(\d)/g, "$1$3 $2$4")
    .split(/[^\p{L}\p{N}]+/u)
    .map(tagKey)
    .filter(Boolean);

// Même devinette que Rust (guess_kind), pour un field pas encore enregistré.
export function guessKind(name, defaultValue = "") {
  const words = wordsOf(name);
  const has = (list) => words.some((w) => list.includes(w));
  // Débuts de mots : « IDEnti », « identifiant » ; « quantites »…
  const starts = (list) => words.some((w) => list.some((p) => w.startsWith(p)));
  const def = defaultValue.trim();
  if (has(["tel", "phone", "mobile", "gsm", "portable"]) || starts(["msisdn", "telephon"])) return "telephone";
  if (has(["jour", "day", "debut", "fin", "start", "end", "expire", "naissance"]) || starts(["date", "expiration", "echeance"]))
    return "date";
  if (
    has([
      "prix", "price", "somme", "solde", "balance", "nombre", "nb", "qte", "qty", "count", "valeur", "value", "taux", "rate",
      "frais", "fee", "fees", "bonus", "commission", "seuil", "limite", "limit", "plafond", "duree",
    ]) ||
    starts(["montant", "amount", "quantit", "pourcent", "percent"])
  )
    return "nombre";
  if (has(["id", "extid", "code", "ref", "cle", "key", "uuid"]) || starts(["ident", "matricul", "referen"])) return "identifiant";
  if (def && !checkValue("date", def)) return "date";
  if (def && !checkValue("nombre", def)) return "nombre";
  return "texte";
}

const isDate = (v) => {
  const m = /^(\d{2})\/(\d{2})\/(\d{4})$/.exec(v);
  if (!m) return false;
  const [d, mo, y] = [+m[1], +m[2], +m[3]];
  return mo >= 1 && mo <= 12 && d >= 1 && d <= new Date(y, mo, 0).getDate();
};

// Même contrôle que Rust (check_value) : null si la valeur convient, sinon ce qui est attendu.
export function checkValue(kind, value) {
  const v = value.trim();
  if (!v) return null;
  const ok = {
    nombre: /^-?\d+(\.\d+)?$/.test(v),
    date: isDate(v),
    identifiant: /^[A-Za-z0-9_.-]+$/.test(v),
    telephone: /^\+?\d{8,15}$/.test(v),
  }[kind];
  if (ok === undefined || ok) return null;
  return {
    nombre: "nombre attendu, ex. 10000",
    date: "date attendue au format JJ/MM/AAAA",
    identifiant: "lettres, chiffres, - _ ou . sans espace",
    telephone: "numéro attendu, 8 à 15 chiffres",
  }[kind];
}

// Lettre de colonne façon tableur : 0 → A, 26 → AA.
export function colLetter(i) {
  let s = "";
  for (let n = i + 1; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}

// « 4 colonnes · 1 field à saisir », pour présenter un type.
export function typeSummary(type) {
  const n = type.fields.filter((f) => f.inLine).length;
  const f = type.fields.filter((f) => f.source === "saisie").length;
  const fields = f ? `${f} ${f > 1 ? "fields" : "field"} à saisir` : "aucun field à saisir";
  return `${n} ${n > 1 ? "colonnes" : "colonne"} · ${fields}`;
}

// Découpe un texte autour de chaque occurrence de `needle` : [{ text, hit }, …].
export function markParts(text, needle) {
  if (!needle) return [{ text, hit: false }];
  const parts = [];
  let from = 0;
  for (let i = text.indexOf(needle); i >= 0; i = text.indexOf(needle, from)) {
    if (i > from) parts.push({ text: text.slice(from, i), hit: false });
    parts.push({ text: needle, hit: true });
    from = i + needle.length;
  }
  if (from < text.length) parts.push({ text: text.slice(from), hit: false });
  return parts;
}

// Retrouve le numéro d'une ligne enregistrée grâce à sa ligne modèle (où il vaut « ‹numéro› »),
// sur le premier élément qui contient exactement une fois la balise. null si introuvable.
export function numberIn(line, template, placeholder = "‹numéro›") {
  const t = splitRaw(template);
  const f = splitRaw(line);
  if (t.length !== f.length) return null;
  for (let k = 0; k < t.length; k++) {
    const parts = t[k].split(placeholder);
    if (parts.length !== 2) continue;
    const [pre, post] = parts;
    const value = f[k];
    if (value.length > pre.length + post.length && value.startsWith(pre) && value.endsWith(post)) {
      return value.slice(pre.length, value.length - post.length);
    }
  }
  return null;
}

// « C:\Users\aloys\Documents\MMC Batch » → « C:\…\Documents\MMC Batch » (le chemin complet va en infobulle).
export function shortPath(path) {
  const parts = path.split("\\");
  return parts.length > 3 ? [parts[0], "…", ...parts.slice(-2)].join("\\") : path;
}
