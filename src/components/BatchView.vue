<script setup>
// « Nouveau lot », façon requête Power Query, en trois écrans :
// 1. Source : bouton d'import au centre, puis aperçu du fichier et « Valider » ;
// 2. Type d'en-tête : bouton de sélection au centre ; le type choisi s'écrit en haut (zone Nom + barre fx) ;
// 3. Champs : grille éditable (toute la colonne suit la valeur saisie) et volet des champs
//    (type de la valeur, deviné au besoin ; obligatoire ou facultatif), puis « Générer le CSV ».
// Rust construit l'aperçu et le fichier ; ici on affiche et on appelle les commandes (invoke).
import { computed, nextTick, onActivated, onBeforeUnmount, onDeactivated, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import AppIcon from "./AppIcon.vue";
import KindGlyph from "./KindGlyph.vue";
import KindMenu from "./KindMenu.vue";
import SheetGrid from "./SheetGrid.vue";
import { checkValue, fieldPlaces, kindLabel, shortPath, splitRaw, tagKey, tagsIn, typeSummary } from "../csvDisplay.js";

const props = defineProps({
  types: { type: Array, default: () => [] },
  typesError: { type: String, default: "" },
  outputDir: { type: String, default: "" },
  typeRequest: { type: Object, default: () => ({ id: null, seq: 0 }) }, // « Utiliser pour un lot »
});
const emit = defineEmits(["types-changed", "generated", "choose-folder", "open-types", "source"]);

const TYPE_KEY = "mmc-batch:type";
const PREVIEW_DELAY = 180; // ms
const PREVIEW_ROWS = 40;

const fmt = new Intl.NumberFormat("fr-FR");
const hour = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit" });
const plural = (n, one, many) => `${fmt.format(n)} ${n > 1 ? many : one}`;
const isNumberTag = (tag) => ["numero", "parametreprincipal", "msisdn"].includes(tagKey(tag));
const isTotalTag = (tag) => ["total", "nombre", "lignes", "nombredelignes"].includes(tagKey(tag));

function readStored(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(key)) ?? fallback;
  } catch {
    return fallback;
  }
}

const step = ref(1);

// --- 1. Source : Excel (une feuille au choix) ou CSV ; les lignes restent côté Rust ---------
// sheet : { sheets, sheet, headerRow, columns: [{ letter, name, tag, sample }], hasKey, firstRows, count, … }
const sheet = ref(null);
const filePath = ref("");
const fileName = computed(() => filePath.value.split(/[\\/]/).pop());
const importing = ref(false);
const importError = ref("");
const keyColumn = computed(() => sheet.value?.columns.find((c) => c.tag === "numero") ?? null);
const canStep2 = computed(() => !!sheet.value?.hasKey && sheet.value.count > 0);

async function importFile() {
  if (importing.value) return;
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Excel ou CSV", extensions: ["xlsx", "xlsm", "xls", "xlsb", "ods", "csv", "txt"] }],
  });
  if (path) await loadFile(path); // sinon boîte de dialogue annulée
}

// En cas d'erreur, le fichier précédent reste chargé.
async function loadFile(path, sheetName, headerRow) {
  importing.value = true;
  importError.value = "";
  try {
    sheet.value = await invoke("read_file", { path, sheet: sheetName ?? null, headerRow: headerRow ?? null });
    filePath.value = path;
    step.value = 1;
  } catch (e) {
    importError.value = String(e);
  } finally {
    importing.value = false;
  }
}

const chooseSheet = (name) => loadFile(filePath.value, name);
const chooseHeaderRow = (value) => loadFile(filePath.value, sheet.value.sheet, value);

// Aperçu de la source : les premières lignes telles que lues, numéros en jaune pâle.
const sourceColumns = computed(() =>
  (sheet.value?.columns ?? []).map((c) => ({
    title: c.name || `Colonne ${c.letter}`,
    kind: c.tag === "numero" ? "telephone" : "texte",
    from: c.tag === "numero" ? "file" : "fixed",
    param: null,
  })),
);
const sourceRows = computed(() => {
  const s = sheet.value;
  if (!s) return [];
  const key = s.columns.findIndex((c) => c.tag === "numero");
  const first = s.headerRow ? 2 : 1;
  const rows = s.firstRows.map((cells, i) => ({
    id: `s${i}`,
    no: first + i,
    type: "data",
    cells: cells.map((text, c) => (c === key ? [{ text, kind: "excel" }] : text)),
  }));
  if (s.count > rows.length) rows.push({ id: "more", type: "more", text: `… et ${plural(s.count - rows.length, "autre ligne", "autres lignes")}` });
  return rows;
});

// Canevas vide derrière le bouton d'import : une feuille qui attend sa source.
const blankColumns = Array.from({ length: 9 }, () => ({ title: "", kind: null, from: "none", param: null }));
const blankRows = Array.from({ length: 22 }, (_, i) => ({ id: `b${i}`, no: i + 1, type: "data", cells: [] }));

// --- 2. Type d'en-tête -------------------------------------------------------------
const typeId = ref(readStored(TYPE_KEY, null));
const currentType = computed(() => props.types.find((t) => t.id === typeId.value) ?? null);
const canStep3 = computed(() => canStep2.value && !!currentType.value);
const typeMenu = ref(false);
const typeMenuEl = ref(null);

function chooseType(id) {
  typeId.value = id;
  typeMenu.value = false;
  try {
    localStorage.setItem(TYPE_KEY, JSON.stringify(id));
  } catch {
    // stockage indisponible : le choix vaut pour la session
  }
}

// La liste s'ouvre vers le haut quand la place manque en dessous du bouton.
const menuUp = ref(false);

async function toggleTypeMenu(e) {
  typeMenu.value = !typeMenu.value;
  if (!typeMenu.value) return;
  const r = e?.currentTarget?.getBoundingClientRect();
  menuUp.value = !!r && window.innerHeight - r.bottom < 300 && r.top > window.innerHeight - r.bottom;
  await nextTick();
  (typeMenuEl.value?.querySelector('[aria-selected="true"]') ?? typeMenuEl.value?.querySelector("[role=option]"))?.focus();
}

function onTypeMenuKey(e) {
  if (!typeMenuEl.value) return; // liste déjà refermée (Entrée vient de choisir un type)
  const items = [...typeMenuEl.value.querySelectorAll("[role=option]")];
  const i = items.indexOf(document.activeElement);
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
  } else if (e.key === "Escape") {
    typeMenu.value = false;
  }
}

function onDocDown(e) {
  if (typeMenu.value && !e.target.closest?.(".type-picker")) typeMenu.value = false;
}

// --- 3. Champs : valeurs saisies, par type (retrouvées en revenant à un type) ----------------
const values = reactive({}); // id du type → { clé du champ → valeur }
watch(
  currentType,
  (t) => {
    if (!t) return;
    const v = (values[t.id] ??= {});
    for (const f of t.fields) if (!(f.key in v)) v[f.key] = f.defaultValue ?? "";
  },
  { immediate: true },
);
const fieldValues = computed(() => (currentType.value ? values[currentType.value.id] ?? {} : {}));
const fields = computed(() => currentType.value?.fields ?? []);

const params = computed(() =>
  fields.value.map((f) => ({ key: f.key, label: f.label, value: fieldValues.value[f.key] ?? "", required: f.required, kind: f.kind })),
);
const valueList = computed(() => params.value.map((p) => p.value));
const problemOf = (p) => checkValue(p.kind, p.value);
// Tout de suite, sans attendre l'aperçu (Ctrl+S juste après la saisie) ; Rust refuse de toute façon d'écrire.
const missing = computed(() => params.value.filter((p) => p.required && !p.value.trim()).map((p) => p.label || p.key));
const invalid = computed(() => params.value.filter((p) => problemOf(p)).map((p) => `${p.label} (${problemOf(p)})`));
const filled = computed(() => params.value.filter((p) => p.value.trim()).length);

const activeParam = ref(-1); // champ survolé ou en cours de saisie : surligné dans la grille
const flash = ref(null);
let flashSeq = 0;
let flashTimer = 0;

// Valeur saisie (volet ou cellule) ; une fois la frappe posée, la colonne s'allume de haut en bas.
function setValue(index, value) {
  const f = fields.value[index];
  if (!f) return;
  fieldValues.value[f.key] = value;
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => (flash.value = { param: index, seq: ++flashSeq }), 420);
}

// Type de la valeur ou caractère obligatoire changés ici : enregistrés dans le type d'en-tête.
const kindMenu = ref(null); // { param, anchor }
const typeNotice = ref("");

async function updateField(index, patch) {
  const t = currentType.value;
  if (!t) return;
  const spec = { ...t, fields: t.fields.map((f, i) => (i === index ? { ...f, ...patch } : f)) };
  kindMenu.value = null;
  try {
    await invoke("save_type", { spec });
    const f = spec.fields[index];
    typeNotice.value =
      "kind" in patch
        ? `« ${f.label} » est maintenant de type ${kindLabel(f.kind).toLowerCase()} (enregistré dans le type ${t.name}).`
        : `« ${f.label} » est maintenant ${f.required ? "obligatoire" : "facultatif"} (enregistré dans le type ${t.name}).`;
    emit("types-changed");
  } catch (e) {
    typeNotice.value = "";
    generateError.value = String(e);
  }
}

// --- Aperçu (debounce, réponses périmées ignorées) ----------------------------------
const payload = computed(() =>
  currentType.value
    ? {
        header: currentType.value.header,
        includeHeader: true,
        fields: currentType.value.columns.map((text) => ({ text })),
        params: params.value,
      }
    : null,
);
const preview = ref(null);
const previewError = ref("");
let previewTimer = 0;
let previewSeq = 0;

async function refreshPreview() {
  const seq = ++previewSeq;
  if (!payload.value) {
    preview.value = null;
    previewError.value = "";
    return;
  }
  try {
    const result = await invoke("build_preview", { ...payload.value, limit: PREVIEW_ROWS });
    if (seq === previewSeq) {
      preview.value = result;
      previewError.value = "";
    }
  } catch (e) {
    if (seq === previewSeq) previewError.value = String(e);
  }
}

// --- Grille du lot : colonnes du type, ligne d'en-tête puis lignes de données --------
const headerTemplates = computed(() => (currentType.value?.header.trim() ? splitRaw(currentType.value.header) : []));

const gridColumns = computed(() => {
  const t = currentType.value;
  if (!t) return [];
  const n = Math.max(t.columns.length, headerTemplates.value.length);
  return Array.from({ length: n }, (_, i) => {
    const template = t.columns[i];
    if (template == null) return { title: "", kind: null, from: "none", param: null };
    const tags = tagsIn(template);
    const whole = tags.length === 1 && template.trim() === `{{${tags[0]}}}`;
    if (whole && isNumberTag(tags[0])) return { title: keyColumn.value?.name || "numéro", kind: "telephone", from: "file", param: null };
    if (whole) {
      const index = t.fields.findIndex((f) => tagKey(f.key) === tagKey(tags[0]));
      const f = t.fields[index];
      if (f) return { title: f.label, kind: f.kind, from: "field", param: index };
    }
    if (tags.length) return { title: template, kind: "texte", from: "mixed", param: null };
    return { title: template || "(vide)", kind: "texte", from: "fixed", param: null };
  });
});

const gridRows = computed(() => {
  const p = preview.value;
  if (!p) return [];
  const rows = [];
  let no = 1;
  if (p.headerParts) rows.push({ id: "h", no: no++, type: "header", cells: p.headerParts, templates: headerTemplates.value });
  const columns = currentType.value?.columns ?? [];
  const lines = p.lines.length ? p.lines : p.sample ? [p.sample] : [];
  lines.forEach((line, i) =>
    rows.push({ id: `l${i}`, no: no++, type: p.lines.length ? "data" : "sample", cells: line.fields, templates: columns }),
  );
  const more = p.dataLines - p.lines.length;
  if (more > 0) rows.push({ id: "more", type: "more", text: `… et ${plural(more, "autre ligne", "autres lignes")}, jusqu'à la ligne ${fmt.format(p.totalLines)}` });
  return rows;
});

// Écran 2 : un extrait suffit ; l'écran 3 montre les 40 premières lignes.
const typePreviewRows = computed(() => {
  const rows = gridRows.value.filter((r) => r.type !== "more").slice(0, 9);
  const shown = rows.filter((r) => r.type === "data").length;
  const total = preview.value?.dataLines ?? 0;
  return total > shown ? [...rows, { id: "more", type: "more", text: `… et ${plural(total - shown, "autre ligne", "autres lignes")}` }] : rows;
});

// Cellule sélectionnée : sa formule (la colonne du type) s'affiche dans la barre fx.
const selected = ref(null); // { ref, template, param }

// Morceaux d'un modèle pour la barre fx : balises colorées selon leur provenance.
function templateParts(template) {
  const parts = [];
  let last = 0;
  for (const m of template.matchAll(/\{\{(.*?)\}\}/gs)) {
    if (m.index > last) parts.push({ text: template.slice(last, m.index), kind: "text" });
    const kind = isNumberTag(m[1]) ? "excel" : isTotalTag(m[1]) ? "text" : "value";
    parts.push({ text: m[0], kind });
    last = m.index + m[0].length;
  }
  if (last < template.length) parts.push({ text: template.slice(last), kind: "text" });
  return parts;
}

// --- Génération ---------------------------------------------------------------------
const generating = ref(false);
const saved = ref(null); // { path, totalLines, at }
const generateError = ref("");
const now = ref(new Date());
const pad = (n) => String(n).padStart(2, "0");
const datedName = (d) =>
  `export_${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}_${pad(d.getHours())}h${pad(d.getMinutes())}.csv`;
const canGenerate = computed(
  () =>
    step.value === 3 &&
    canStep3.value &&
    !generating.value &&
    !previewError.value &&
    !!props.outputDir &&
    !missing.value.length &&
    !invalid.value.length,
);

async function generateCsv() {
  if (!canGenerate.value) return;
  generating.value = true;
  generateError.value = "";
  try {
    const result = await invoke("generate_csv", {
      ...payload.value,
      typeName: currentType.value.name,
      folder: props.outputDir,
      fileName: datedName(new Date()),
      sourceName: fileName.value,
    });
    saved.value = { ...result, at: new Date() };
    emit("generated");
  } catch (e) {
    generateError.value = String(e);
  } finally {
    generating.value = false;
  }
}

// Nouveau lot : on repart du fichier ; le type reste choisi, ses valeurs reviennent aux valeurs par défaut.
function newBatch() {
  sheet.value = null;
  filePath.value = "";
  saved.value = null;
  if (currentType.value) delete values[currentType.value.id];
  step.value = 1;
}

// Toute modification retire le message « Fichier enregistré » devenu faux et rafraîchit l'aperçu.
watch(
  [sheet, payload],
  () => {
    saved.value = null;
    generateError.value = "";
    clearTimeout(previewTimer);
    previewTimer = setTimeout(refreshPreview, PREVIEW_DELAY);
  },
  { immediate: true },
);
watch(typeId, () => {
  typeNotice.value = "";
  selected.value = null;
});

// Le fichier chargé est prêté à l'onglet des types (« Depuis le fichier chargé »).
watch(sheet, (s) => emit("source", { sheet: s, fileName: fileName.value }));

// Type choisi depuis l'onglet des types : on le prend et on avance aussi loin que possible.
watch(
  () => props.typeRequest.seq,
  () => {
    if (props.typeRequest.id == null) return;
    chooseType(props.typeRequest.id);
    step.value = canStep2.value ? 3 : 1;
  },
);

// --- Étapes appliquées (volet) -------------------------------------------------------
const steps = computed(() => [
  {
    n: 1,
    name: "Source",
    detail: sheet.value ? `${fileName.value} · ${plural(sheet.value.count, "ligne", "lignes")}` : "aucun fichier",
    done: canStep2.value,
    reachable: true,
  },
  {
    n: 2,
    name: "Type d'en-tête",
    detail: currentType.value?.name ?? "à choisir",
    done: !!currentType.value,
    reachable: canStep2.value,
  },
  {
    n: 3,
    name: "Champs renseignés",
    detail: fields.value.length ? `${filled.value} / ${fields.value.length} renseignés` : "aucun champ",
    done: canStep3.value && !missing.value.length && !invalid.value.length,
    reachable: canStep3.value,
  },
]);

function goTo(n) {
  if ((n === 2 && !canStep2.value) || (n === 3 && !canStep3.value)) return;
  step.value = n;
  selected.value = null;
  typeMenu.value = false;
}

// --- Barre de message (sous la barre fx), comme les bandeaux de Power Query ----------
const message = computed(() => {
  if (step.value === 1) {
    if (importError.value) return { tone: "error", text: importError.value };
    if (sheet.value && !sheet.value.hasKey)
      return { tone: "error", text: "Aucune colonne de numéros reconnue : il faut une colonne « msisdn » ou une colonne de numéros." };
    return null;
  }
  if (props.typesError) return { tone: "error", text: props.typesError };
  if (step.value === 2) return null;
  if (previewError.value) return { tone: "error", text: previewError.value };
  if (generateError.value) return { tone: "error", text: generateError.value };
  if (saved.value) return { tone: "ok", saved: saved.value };
  if (typeNotice.value) return { tone: "info", text: typeNotice.value };
  if (missing.value.length || invalid.value.length) return { tone: "warn" };
  return null;
});

// --- Barre d'état ----------------------------------------------------------------------
const status = computed(() => {
  const s = sheet.value;
  if (step.value === 1) {
    if (importing.value) return { left: "Lecture du fichier…" };
    if (!s) return { left: "En attente d'un fichier" };
    return {
      left: s.hasKey ? "Prêt" : "Colonne de numéros introuvable",
      bad: !s.hasKey,
      right: [plural(s.count, "ligne", "lignes"), plural(s.columns.length, "colonne", "colonnes"), plural(s.duplicatesRemoved, "doublon retiré", "doublons retirés")],
    };
  }
  const t = currentType.value;
  if (step.value === 2) {
    if (!t) return { left: "Choisissez un type d'en-tête" };
    return { left: `Type ${t.name}`, right: [plural(t.columns.length, "colonne", "colonnes"), plural(t.fields.length, "champ", "champs")] };
  }
  const p = preview.value;
  const issues = missing.value.length + invalid.value.length;
  return {
    left: generating.value
      ? "Écriture du fichier…"
      : issues
        ? `${plural(issues, "champ", "champs")} à revoir`
        : "Prêt à générer",
    bad: !!issues && !generating.value,
    right: p ? [plural(p.totalLines, "ligne", "lignes"), plural(t.columns.length, "colonne", "colonnes"), "UTF-8", "CRLF"] : [],
  };
});

// --- Raccourcis : Ctrl+O importer, Ctrl+S générer, Ctrl+Entrée valider -------------------
function onKeydown(e) {
  if (!e.ctrlKey || e.altKey || e.shiftKey) return;
  const key = e.key.toLowerCase();
  if (key === "o") {
    e.preventDefault();
    importFile();
  } else if (key === "s" && step.value === 3) {
    e.preventDefault();
    generateCsv();
  } else if (key === "enter" && step.value < 3) {
    e.preventDefault();
    goTo(step.value + 1);
  }
}

let clock = 0;
onActivated(() => {
  window.addEventListener("keydown", onKeydown);
  document.addEventListener("mousedown", onDocDown);
  clock = setInterval(() => (now.value = new Date()), 20_000);
  now.value = new Date();
});
onDeactivated(() => {
  window.removeEventListener("keydown", onKeydown);
  document.removeEventListener("mousedown", onDocDown);
  clearInterval(clock);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  document.removeEventListener("mousedown", onDocDown);
  clearInterval(clock);
  clearTimeout(previewTimer);
  clearTimeout(flashTimer);
});
</script>

<template>
  <section class="batch" aria-label="Nouveau lot">
    <!-- Ruban : les commandes de l'écran, l'action suivante à droite -->
    <div class="ribbon" role="toolbar" aria-label="Commandes">
      <template v-if="step === 1">
        <button class="rb" type="button" :disabled="importing" aria-keyshortcuts="Control+O" @click="importFile">
          <AppIcon name="import" /><span>{{ sheet ? "Changer de fichier" : "Importer" }}</span>
        </button>
        <template v-if="sheet">
          <span class="rb-sep" aria-hidden="true" />
          <label v-if="sheet.sheets.length > 1" class="rb-field">
            <span>Feuille</span>
            <select class="input select" :value="sheet.sheet" :disabled="importing" @change="chooseSheet($event.target.value)">
              <option v-for="s in sheet.sheets" :key="s" :value="s">{{ s }}</option>
            </select>
          </label>
          <label class="rb-check">
            <input type="checkbox" :checked="sheet.headerRow" :disabled="importing" @change="chooseHeaderRow($event.target.checked)" />
            <span>1<sup>re</sup> ligne = noms des colonnes</span>
          </label>
        </template>
        <span class="rb-fill" />
        <button class="btn btn-primary" type="button" :disabled="!canStep2" aria-keyshortcuts="Control+Enter" @click="goTo(2)">
          Valider<AppIcon name="arrow" />
        </button>
      </template>

      <template v-else-if="step === 2">
        <div v-if="currentType" class="type-picker">
          <button class="rb rb-type" type="button" :aria-expanded="typeMenu" aria-haspopup="listbox" @click="toggleTypeMenu">
            <AppIcon name="table" /><span>Type : <strong>{{ currentType.name }}</strong></span><AppIcon name="caret" />
          </button>
          <ul v-if="typeMenu" ref="typeMenuEl" class="menu type-menu" :class="{ up: menuUp }" role="listbox" aria-label="Types d'en-tête" @keydown="onTypeMenuKey">
            <li
              v-for="t in types"
              :key="t.id"
              class="type-option"
              role="option"
              tabindex="-1"
              :aria-selected="t.id === typeId ? 'true' : 'false'"
              @click="chooseType(t.id)"
              @keydown.enter.prevent="chooseType(t.id)"
            >
              <span class="type-name">{{ t.name }}</span>
              <span class="type-meta">{{ typeSummary(t) }}</span>
              <code class="type-line">{{ t.header || "sans ligne d'en-tête" }}</code>
            </li>
          </ul>
        </div>
        <button class="rb" type="button" @click="emit('open-types', currentType?.id ?? null)">
          <AppIcon name="edit" /><span>Gérer les types</span>
        </button>
        <span class="rb-fill" />
        <button class="btn btn-ghost" type="button" @click="goTo(1)"><AppIcon name="back" />Source</button>
        <button class="btn btn-primary" type="button" :disabled="!canStep3" aria-keyshortcuts="Control+Enter" @click="goTo(3)">
          Valider<AppIcon name="arrow" />
        </button>
      </template>

      <template v-else>
        <button class="rb rb-dest" type="button" :title="`Dossier de sortie : ${outputDir || 'aucun'} — cliquer pour le changer`" @click="emit('choose-folder')">
          <AppIcon name="folder" /><span class="rb-path">{{ outputDir ? shortPath(outputDir) : "Choisir le dossier de sortie…" }}</span>
        </button>
        <span class="rb-file" title="Nom du fichier qui sera écrit"><code>{{ datedName(now) }}</code></span>
        <span class="rb-fill" />
        <button class="btn btn-ghost" type="button" @click="goTo(2)"><AppIcon name="back" />Type</button>
        <button class="btn btn-primary btn-generate" type="button" :disabled="!canGenerate" aria-keyshortcuts="Control+S" @click="generateCsv">
          <AppIcon name="download" />{{ generating ? "Écriture…" : "Générer le CSV" }}<kbd>Ctrl+S</kbd>
        </button>
      </template>
    </div>

    <!-- Barre de formule : à gauche le nom (feuille ou type d'en-tête), à droite son contenu -->
    <div class="fx-bar">
      <span class="name-box" :class="{ empty: step > 1 && !currentType }" :title="step === 1 ? 'Feuille lue' : 'Type d\'en-tête'">
        {{ step === 1 ? sheet?.sheet || "Source" : currentType?.name || "Aucun type" }}
      </span>
      <span class="fx" aria-hidden="true">fx</span>
      <p class="fx-content selectable" :title="step === 1 ? filePath : currentType?.header">
        <template v-if="step === 1">
          <code v-if="filePath">{{ filePath }}</code>
          <span v-else class="fx-quiet">Aucun fichier : importez un Excel ou un CSV.</span>
        </template>
        <template v-else-if="step === 3 && selected">
          <span class="fx-ref">{{ selected.ref }}</span>
          <code v-if="selected.template != null"><template v-for="(part, q) in templateParts(selected.template)" :key="q"><span v-if="part.kind !== 'text'" :class="`t-${part.kind}`">{{ part.text }}</span><template v-else>{{ part.text }}</template></template></code>
          <span v-else class="fx-quiet">(cellule vide)</span>
        </template>
        <template v-else-if="currentType">
          <code v-if="preview?.headerParts"><template v-for="(cell, c) in preview.headerParts" :key="c"><template v-if="c">,<wbr /></template><template v-for="(seg, q) in cell" :key="q"><span v-if="seg.kind !== 'text'" :class="`t-${seg.kind}`">{{ seg.text }}</span><template v-else>{{ seg.text }}</template></template></template></code>
          <span v-else-if="!currentType.header.trim()" class="fx-quiet">Pas de ligne d'en-tête : le fichier commence directement par les lignes.</span>
        </template>
        <span v-else class="fx-quiet">Sélectionnez un type d'en-tête : sa 1<sup>re</sup> ligne s'écrira ici.</span>
      </p>
    </div>

    <div v-if="message" class="msgbar" :class="`m-${message.tone}`" :role="message.tone === 'error' ? 'alert' : 'status'">
      <AppIcon :name="message.tone === 'ok' ? 'check' : 'alert'" />
      <p v-if="message.saved">
        <strong>Fichier enregistré</strong> à {{ hour.format(message.saved.at) }} · {{ plural(message.saved.totalLines, "ligne", "lignes") }} · ajouté à
        l'historique · <code :title="message.saved.path">{{ message.saved.path.split("\\").pop() }}</code>
      </p>
      <p v-else-if="message.tone === 'warn'">
        <template v-if="missing.length"><strong>À renseigner :</strong> {{ missing.join(", ") }}</template
        ><template v-if="missing.length && invalid.length"> · </template
        ><template v-if="invalid.length"><strong>À corriger :</strong> {{ invalid.join(", ") }}</template>
      </p>
      <p v-else>{{ message.text }}</p>
      <button v-if="message.saved" class="btn btn-secondary btn-small" type="button" @click="newBatch">Nouveau lot</button>
      <button v-else-if="message.tone === 'info'" class="btn btn-ghost btn-small" type="button" @click="typeNotice = ''">OK</button>
    </div>

    <div class="work">
      <!-- Canevas -->
      <div class="canvas">
        <!-- 1. Source -->
        <template v-if="step === 1">
          <div v-if="!sheet" class="center-stage">
            <SheetGrid class="ghost-sheet" :columns="blankColumns" :rows="blankRows" label="Feuille vide" aria-hidden="true" tabindex="-1" />
            <div class="center">
              <AppIcon name="import" class="center-icon" />
              <h2 class="center-title">Importez le fichier des numéros</h2>
              <p class="center-text">Excel ou CSV. Chaque numéro de la colonne « msisdn » donnera une ligne du fichier.</p>
              <button class="btn btn-primary btn-big" type="button" :disabled="importing" aria-keyshortcuts="Control+O" @click="importFile">
                <AppIcon name="import" />{{ importing ? "Lecture…" : "Importer un fichier" }}<kbd>Ctrl+O</kbd>
              </button>
            </div>
          </div>
          <SheetGrid v-else :columns="sourceColumns" :rows="sourceRows" :label="`Aperçu de ${fileName}`" />
        </template>

        <!-- 2. Type d'en-tête -->
        <template v-else-if="step === 2">
          <div v-if="!currentType" class="center-stage">
            <div class="center">
              <AppIcon name="table" class="center-icon" />
              <h2 class="center-title">Choisissez le type d'en-tête</h2>
              <p class="center-text">Il fixe la 1<sup>re</sup> ligne du fichier, les colonnes de chaque ligne et les champs à renseigner.</p>
              <div v-if="types.length" class="type-picker">
                <button class="btn btn-primary btn-big" type="button" :aria-expanded="typeMenu" aria-haspopup="listbox" @click="toggleTypeMenu">
                  Sélectionner le type d'en-tête<AppIcon name="caret" />
                </button>
                <ul v-if="typeMenu" ref="typeMenuEl" class="menu type-menu centered" :class="{ up: menuUp }" role="listbox" aria-label="Types d'en-tête" @keydown="onTypeMenuKey">
                  <li
                    v-for="t in types"
                    :key="t.id"
                    class="type-option"
                    role="option"
                    tabindex="-1"
                    aria-selected="false"
                    @click="chooseType(t.id)"
                    @keydown.enter.prevent="chooseType(t.id)"
                  >
                    <span class="type-name">{{ t.name }}</span>
                    <span class="type-meta">{{ typeSummary(t) }}</span>
                    <code class="type-line">{{ t.header || "sans ligne d'en-tête" }}</code>
                  </li>
                </ul>
              </div>
              <button v-else class="btn btn-primary btn-big" type="button" @click="emit('open-types', null, true)">
                <AppIcon name="plus" />Créer un type d'en-tête
              </button>
              <p class="center-quiet">{{ plural(types.length, "type enregistré", "types enregistrés") }}</p>
            </div>
          </div>
          <div v-else class="type-preview">
            <SheetGrid :columns="gridColumns" :rows="typePreviewRows" :label="`Aperçu du type ${currentType.name}`" />
          </div>
        </template>

        <!-- 3. Champs : la grille du fichier, éditable -->
        <SheetGrid
          v-else
          :columns="gridColumns"
          :rows="gridRows"
          :values="valueList"
          editable
          :active-param="activeParam"
          :flash="flash"
          label="Fichier à générer : cliquez une cellule bleue pour changer sa valeur"
          @edit="setValue"
          @select="selected = $event"
          @pick-kind="(param, anchor) => (kindMenu = { param, anchor })"
          @hover-param="activeParam = $event"
        />
      </div>

      <!-- Volet « Paramètres de la requête » -->
      <aside class="pane" aria-label="Paramètres de la requête">
        <h2 class="pane-title caps">Paramètres de la requête</h2>

        <section class="pane-sec">
          <h3 class="sec-title caps">Étapes appliquées</h3>
          <ol class="steps">
            <li v-for="s in steps" :key="s.n">
              <button
                class="step"
                :class="{ on: step === s.n, done: s.done }"
                type="button"
                :disabled="!s.reachable"
                :aria-current="step === s.n ? 'step' : undefined"
                @click="goTo(s.n)"
              >
                <span class="step-mark" aria-hidden="true"><AppIcon v-if="s.done" name="check" /><template v-else>{{ s.n }}</template></span>
                <span class="step-text">
                  <span class="step-name">{{ s.name }}</span>
                  <span class="step-detail">{{ s.detail }}</span>
                </span>
              </button>
            </li>
          </ol>
        </section>

        <!-- 1 : le paramètre principal -->
        <section v-if="step === 1 && sheet" class="pane-sec">
          <h3 class="sec-title caps">Paramètre principal</h3>
          <dl v-if="keyColumn" class="props">
            <div><dt>Balise</dt><dd><code class="tag-file" v-pre>{{numero}}</code></dd></div>
            <div><dt>Colonne</dt><dd>{{ keyColumn.letter }}<template v-if="keyColumn.name"> · {{ keyColumn.name }}</template></dd></div>
            <div><dt>Exemple</dt><dd class="mono">{{ keyColumn.sample }}</dd></div>
            <div><dt>Lignes</dt><dd>{{ fmt.format(sheet.count) }}</dd></div>
            <div><dt>Doublons retirés</dt><dd>{{ fmt.format(sheet.duplicatesRemoved) }}</dd></div>
            <div v-if="sheet.skipped"><dt>Sans numéro</dt><dd>{{ fmt.format(sheet.skipped) }} ignorées</dd></div>
          </dl>
          <p v-else class="pane-note bad">Aucune colonne de numéros dans ce fichier.</p>
        </section>

        <!-- 2 et 3 : les champs du type choisi, à remplir ici dès le choix du type (ou dans la grille à l'écran 3) -->
        <section v-if="step >= 2 && currentType" class="pane-sec pane-fields">
          <h3 class="sec-title caps">
            Champs à remplir<span v-if="fields.length" class="sec-count">{{ filled }} / {{ fields.length }}</span>
          </h3>
          <p v-if="step === 2 && currentType.description" class="pane-note type-desc">{{ currentType.description }}</p>
          <p v-if="!fields.length" class="pane-note">Ce type n'a aucun champ à renseigner : vérifiez la grille, puis générez.</p>
          <ul class="fields">
            <li
              v-for="(f, i) in fields"
              :key="f.key"
              class="field"
              :class="{
                lit: i === activeParam,
                bad: (f.required && !params[i].value.trim()) || !!problemOf(params[i]),
              }"
              @mouseenter="activeParam = i"
              @mouseleave="activeParam = -1"
              @focusin="activeParam = i"
              @focusout="activeParam = -1"
            >
              <div class="field-top">
                <button
                  class="kind-btn"
                  type="button"
                  :title="`Type : ${kindLabel(f.kind)}${f.guessed ? ' (deviné)' : ''} — cliquer pour le changer`"
                  :aria-label="`Type de ${f.label} : ${kindLabel(f.kind)}${f.guessed ? ', deviné' : ''}. Changer`"
                  @click="kindMenu = { param: i, anchor: $event.currentTarget }"
                >
                  <KindGlyph :kind="f.kind" />
                </button>
                <label class="field-label" :for="`champ-${f.key}`">{{ f.label }}</label>
                <button
                  class="req"
                  :class="{ on: f.required }"
                  type="button"
                  :aria-pressed="f.required"
                  :title="f.required ? 'Obligatoire : la génération attend cette valeur' : 'Facultatif : peut rester vide'"
                  @click="updateField(i, { required: !f.required })"
                >
                  {{ f.required ? "Obligatoire" : "Facultatif" }}
                </button>
              </div>
              <input
                :id="`champ-${f.key}`"
                class="input"
                type="text"
                spellcheck="false"
                autocomplete="off"
                :value="params[i].value"
                :required="f.required"
                :aria-invalid="!!problemOf(params[i]) || undefined"
                :placeholder="f.required ? 'À renseigner' : 'Vide si inutile'"
                @input="setValue(i, $event.target.value)"
              />
              <p class="field-note">
                <span v-if="problemOf(params[i])" class="note-bad">{{ problemOf(params[i]) }}</span>
                <template v-else>{{ fieldPlaces(f.key, currentType.header, currentType.columns) }}</template>
                <span v-if="f.guessed" class="note-guess"> · type deviné</span>
              </p>
            </li>
          </ul>
        </section>
      </aside>
    </div>

    <!-- Barre d'état, en petites capitales comme celle de Power Query -->
    <footer class="status caps">
      <span :class="{ 'st-bad': status.bad }">{{ status.left }}</span>
      <span class="st-right">
        <template v-for="(item, i) in status.right ?? []" :key="i"><template v-if="i"> · </template>{{ item }}</template>
      </span>
    </footer>

    <KindMenu
      v-if="kindMenu"
      :anchor="kindMenu.anchor"
      :kind="fields[kindMenu.param]?.kind"
      :guessed="fields[kindMenu.param]?.guessed"
      :title="fields[kindMenu.param]?.label"
      @pick="(kind) => updateField(kindMenu.param, { kind, guessed: false })"
      @close="kindMenu = null"
    />
  </section>
</template>

<style scoped>
/* Ruban, barre fx, bandeau éventuel, puis canevas + volet qui prennent toute la hauteur, barre d'état. */
.batch {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.batch > * {
  flex: none;
}

.batch > .work {
  flex: 1;
}

/* --- Ruban ------------------------------------------------------------- */
.ribbon {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 40px;
  padding: 4px 8px;
  border-bottom: 1px solid var(--border);
  background: var(--ribbon);
}

.rb {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid transparent;
  border-radius: var(--radius);
  background: none;
  color: var(--text);
  font-size: 13px;
  white-space: nowrap;
  transition: background-color 120ms ease-out;
}

.rb .icon {
  color: var(--link);
}

.rb:hover:not(:disabled) {
  background: var(--tint-50);
}

.rb:disabled {
  color: var(--disabled-text);
  cursor: not-allowed;
}

.rb-type strong {
  font-weight: 700;
}

.rb-sep {
  align-self: stretch;
  width: 1px;
  margin: 2px 4px;
  background: var(--border);
}

.rb-fill {
  flex: 1;
}

.rb-field,
.rb-check {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--text-2);
  font-size: 12.5px;
  white-space: nowrap;
}

.rb-field .select {
  width: auto;
  min-width: 120px;
  max-width: 220px;
  height: 28px;
}

.rb-dest {
  min-width: 0;
  max-width: 320px;
}

.rb-path {
  overflow: hidden;
  color: var(--text);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rb-file {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  white-space: nowrap;
}

.rb-file code {
  overflow: hidden;
  color: var(--text-2);
  font: 12px var(--font-mono);
  text-overflow: ellipsis;
}

.btn-generate {
  height: 30px;
  padding: 0 14px;
}

.btn-generate kbd {
  margin-left: 2px;
}

/* --- Barre de formule ---------------------------------------------------- */
.fx-bar {
  display: grid;
  grid-template-columns: minmax(120px, 200px) auto minmax(0, 1fr);
  align-items: stretch;
  min-height: 28px;
  border-bottom: 1px solid var(--border);
  background: var(--ribbon);
}

.name-box {
  display: flex;
  align-items: center;
  overflow: hidden;
  padding: 0 10px;
  border-right: 1px solid var(--border);
  color: var(--text);
  font-weight: 700;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.name-box.empty {
  color: var(--text-3);
  font-weight: 400;
}

.fx {
  display: grid;
  place-items: center;
  padding: 0 12px;
  border-right: 1px solid var(--border);
  color: var(--text-3);
  font-family: Cambria, Georgia, serif;
  font-style: italic;
  font-size: 14px;
}

.fx-content {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 4px 12px;
  overflow: hidden;
}

.fx-content code {
  overflow: hidden;
  font: 12.5px/1.5 var(--font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fx-quiet {
  color: var(--text-3);
  font-style: italic;
}

.fx-ref {
  flex: none;
  padding: 0 6px;
  border-radius: 3px;
  background: var(--tint-100);
  color: var(--text);
  font-size: 11.5px;
  font-weight: 700;
}

.t-excel {
  color: var(--file-text);
  font-weight: 700;
}

.t-value {
  color: var(--input-text);
  font-weight: 600;
}

.t-missing,
.t-invalid {
  color: var(--bad-text);
  font-weight: 600;
}

/* --- Barre de message --------------------------------------------------- */
.msgbar {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 36px;
  padding: 5px 12px;
  border-bottom: 1px solid;
  font-size: 12.5px;
  animation: msg-in 220ms var(--ease-out);
}

.msgbar p {
  flex: 1;
  min-width: 0;
}

.msgbar code {
  font: 12px var(--font-mono);
}

.m-error {
  border-color: var(--danger-line);
  background: var(--danger-50);
  color: var(--danger);
}

.m-ok,
.m-info {
  border-color: var(--ok-line);
  background: var(--ok-fill);
  color: var(--text);
}

.m-ok .icon,
.m-info .icon {
  color: var(--link);
}

.m-warn {
  border-color: var(--border);
  background: var(--surface);
  color: var(--text-2);
}

.m-warn .icon {
  color: var(--bad-text);
}

.m-warn strong {
  color: var(--text);
}

@keyframes msg-in {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
}

/* --- Canevas et volet ------------------------------------------------------- */
.work {
  display: grid;
  grid-template-columns: minmax(0, 1fr) clamp(236px, 28%, 300px);
  min-height: 0;
}

.canvas {
  display: grid;
  min-width: 0;
  min-height: 0;
  background: var(--bg);
}

.canvas > :deep(.grid-wrap),
.type-preview {
  min-height: 0;
}

.type-preview {
  display: grid;
}

/* Écrans vides : l'action au centre, devant une feuille qui attend. */
.center-stage {
  position: relative;
  display: grid;
  place-items: center;
  min-height: 0;
}

.ghost-sheet {
  position: absolute;
  inset: 0;
  overflow: hidden;
  opacity: 0.45;
  pointer-events: none;
  mask-image: radial-gradient(ellipse 70% 65% at 50% 45%, transparent 22%, #000 78%);
}

/* L'action de l'écran, au centre, dans un petit panneau de dialogue posé sur la feuille vide. */
.center {
  position: relative;
  display: grid;
  justify-items: center;
  gap: 8px;
  width: min(400px, 90%);
  padding: 20px 22px 18px;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: var(--surface);
  box-shadow: var(--menu-shadow);
  text-align: center;
}

.center-icon {
  width: 24px;
  height: 24px;
  color: var(--accent);
  stroke-width: 1.6;
}

:root[data-theme="light"] .center-icon {
  color: var(--link);
}

.center-title {
  color: var(--text);
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
}

.center-text {
  color: var(--text-2);
  font-size: 12.5px;
  text-wrap: balance;
}

.center-text code {
  color: var(--file-text);
  font: 600 11.5px var(--font-mono);
}

.btn-big {
  height: 34px;
  margin-top: 6px;
  padding: 0 16px;
  font-size: 13px;
}

.btn-big kbd {
  margin-left: 4px;
}

.center-quiet {
  color: var(--text-3);
  font-size: 12px;
}

/* Liste des types (bouton de sélection) */
.type-picker {
  position: relative;
}

.type-menu {
  top: calc(100% + 4px);
  left: 0;
  width: 360px;
  max-height: 340px;
  margin: 0;
  overflow-y: auto;
  list-style: none;
  text-align: left;
}

.type-menu.centered {
  left: 50%;
  translate: -50% 0;
}

.type-menu.up {
  top: auto;
  bottom: calc(100% + 4px);
}

.type-option {
  display: grid;
  gap: 1px;
  padding: 7px 10px;
  border-radius: 4px;
}

.type-option:hover,
.type-option:focus-visible {
  outline: none;
  background: var(--tint-100);
}

.type-option[aria-selected="true"] {
  background: var(--input-fill);
}

.type-name {
  color: var(--text);
  font-weight: 700;
}

.type-meta {
  color: var(--text-2);
  font-size: 12px;
}

.type-line {
  overflow: hidden;
  color: var(--text-3);
  font: 11.5px/1.5 var(--font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Volet */
.pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow-y: auto;
  border-left: 1px solid var(--border);
  background: var(--surface);
}

.pane-title {
  padding: 12px 14px 8px;
  color: var(--text);
}

.pane-sec {
  padding: 6px 14px 14px;
}

.pane-sec + .pane-sec {
  padding-top: 12px;
  border-top: 1px solid var(--border);
}

.sec-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 8px;
  color: var(--text-3);
}

.sec-count {
  color: var(--text-2);
  letter-spacing: 0;
}

.steps {
  display: grid;
  gap: 2px;
  padding: 0;
  list-style: none;
}

.step {
  display: grid;
  grid-template-columns: 22px minmax(0, 1fr);
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 6px 8px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--text);
  text-align: left;
  transition: background-color 120ms ease-out;
}

.step:hover:not(:disabled) {
  background: var(--tint-50);
}

.step.on {
  background: var(--tint-100);
}

.step:disabled {
  cursor: not-allowed;
}

.step:disabled .step-name,
.step:disabled .step-mark {
  color: var(--disabled-text);
}

.step-mark {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: 1.5px solid var(--border-strong);
  border-radius: 999px;
  color: var(--text-2);
  font-size: 11px;
  font-weight: 700;
}

.step.done .step-mark {
  border-color: var(--accent);
  background: var(--accent);
  color: var(--on-accent);
}

.step-mark .icon {
  width: 13px;
  height: 13px;
  stroke-width: 2.6;
}

.step-text {
  display: grid;
  min-width: 0;
}

.step-name {
  font-weight: 600;
}

.step-detail {
  overflow: hidden;
  color: var(--text-3);
  font-size: 11.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.props {
  display: grid;
  gap: 6px;
  margin: 0;
}

.props div {
  display: flex;
  justify-content: space-between;
  gap: 10px;
}

.props dt {
  color: var(--text-3);
  font-size: 12px;
}

.props dd {
  margin: 0;
  color: var(--text);
  font-size: 12.5px;
  font-weight: 600;
  text-align: right;
}

.mono {
  font-family: var(--font-mono);
}

.tag-file {
  padding: 0 5px;
  border-radius: 3px;
  background: var(--file-fill);
  color: var(--file-text);
  font: 600 11.5px var(--font-mono);
}

.pane-note {
  color: var(--text-2);
  font-size: 12.5px;
}

.type-desc {
  margin-bottom: 8px;
}

.pane-note.bad {
  color: var(--danger);
}

/* Champs à renseigner */
.fields {
  display: grid;
  gap: 4px;
  padding: 0;
  list-style: none;
}

.field {
  display: grid;
  gap: 4px;
  margin: 0 -8px;
  padding: 6px 8px 5px;
  border-radius: 4px;
  transition: background-color 120ms ease-out;
}

.field.lit {
  background: var(--tint-100);
}

.field-top {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.kind-btn {
  display: grid;
  flex: none;
  place-items: center;
  height: 22px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  background: var(--sheet);
  color: var(--input-text);
  transition: border-color 120ms ease-out;
}

.kind-btn:hover {
  border-color: var(--ring);
}

.field-label {
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.req {
  flex: none;
  margin-left: auto;
  padding: 1px 6px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: none;
  color: var(--text-3);
  font-size: 11px;
  font-weight: 600;
  transition:
    border-color 120ms ease-out,
    color 120ms ease-out,
    background-color 120ms ease-out;
}

.req:hover {
  border-color: var(--border-strong);
  color: var(--text);
}

.req.on {
  border-color: var(--accent);
  background: var(--accent);
  color: var(--on-accent);
}

.field .input {
  height: 30px;
}

.field.bad .input {
  border-color: var(--bad-text);
}

.field .input[aria-invalid="true"] {
  background: var(--bad-fill);
}

.field-note {
  overflow: hidden;
  color: var(--text-3);
  font-size: 11.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.note-bad {
  color: var(--bad-text);
  font-weight: 600;
}

.note-guess {
  color: var(--input-text);
}

/* --- Barre d'état ---------------------------------------------------------- */
.status {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  min-height: 24px;
  padding: 4px 12px;
  border-top: 1px solid var(--border);
  background: var(--ribbon);
  color: var(--text-3);
  font-size: 10.5px;
}

.st-bad {
  color: var(--bad-text);
}

.st-right {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 860px) {
  .rb-file {
    display: none;
  }
  .rb span:not(.icon) {
    max-width: 150px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
}
</style>
