<script setup>
// Types d'en-tête. Chaque type décrit un format de lot : une liste de fields, dans l'ordre des colonnes
// du fichier, chacun avec sa description (pour aider la personne qui fera le lot), d'où vient sa valeur
// (saisie au lot, numéro du fichier, texte fixe ou formule) et s'il est obligatoire ; plus la 1re ligne
// du fichier (texte fixe et nombre de lignes, jamais un field). Rust vérifie et enregistre.
import { computed, nextTick, onActivated, onBeforeUnmount, onDeactivated, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import AppIcon from "./AppIcon.vue";
import KindGlyph from "./KindGlyph.vue";
import KindMenu from "./KindMenu.vue";
import {
  NUMBER_TAG,
  SOURCES,
  colLetter,
  guessKind,
  isTotalTag,
  kindLabel,
  parseFieldTable,
  splitRaw,
  tagKey,
  typeSummary,
} from "../csvDisplay.js";

const props = defineProps({
  types: { type: Array, default: () => [] },
  sheet: { type: Object, default: null }, // fichier chargé dans « Nouveau lot », pour en reprendre le format
  fileName: { type: String, default: "" },
  request: { type: Object, default: () => ({ id: null, create: false, seq: 0 }) }, // type à ouvrir
});
const emit = defineEmits(["changed", "use"]);

const PREVIEW_DELAY = 200; // ms

// --- Brouillons : un par type ouvert (modifications gardées en changeant de type ou d'onglet) ---
let uid = 1;
let newCount = 0;
const drafts = reactive({}); // « t12 » (type enregistré) ou « n3 » (nouveau type) → brouillon
const selectedKey = ref(null);
const keyOf = (t) => `t${t.id}`;
const savedOf = (key) => props.types.find((t) => keyOf(t) === key) ?? null;
const cells = (texts) => texts.map((text) => ({ uid: uid++, text }));

const blankField = (f = {}) => ({
  uid: uid++,
  name: "",
  description: "",
  source: "saisie",
  formula: "",
  inLine: true,
  required: false,
  defaultValue: "",
  kind: "",
  guessed: true,
  ...f,
});

function makeDraft(t = {}, linked = false) {
  return {
    id: t.id ?? null,
    name: t.name ?? "",
    description: t.description ?? "",
    // Cellules brutes de la 1re ligne (guillemets éventuels gardés : l'en-tête reste exact).
    headerCells: cells(t.header?.trim() ? splitRaw(t.header) : []),
    fields: (t.fields ?? []).map((f) => blankField(f)),
    // A1 suit le nom du type tant qu'on ne l'a pas changé (nouveau type).
    linked,
  };
}

// Une cellule d'en-tête qui contient une virgule ou un saut de ligne est mise entre guillemets.
const quoted = (text) => (/^".*"$/s.test(text.trim()) || !/[,\r\n]/.test(text) ? text : `"${text.replaceAll('"', '""')}"`);
const headerOf = (d) => d.headerCells.map((c) => quoted(c.text)).join(",");

// Type de la valeur : deviné d'après le nom et la valeur proposée tant qu'il n'a pas été choisi.
function kindOf(f) {
  const guessed = !f.kind || !!f.guessed;
  return { kind: guessed ? guessKind(f.name.trim(), f.defaultValue) : f.kind, guessed };
}

// Type tel qu'envoyé à Rust.
function toSpec(d) {
  return {
    id: d.id,
    name: d.name.trim(),
    description: d.description.trim(),
    header: headerOf(d),
    fields: d.fields.map((f) => ({
      name: f.name.trim(),
      description: f.description.trim(),
      source: f.source,
      formula: f.source === "formule" ? f.formula : "",
      inLine: f.source === "saisie" ? f.inLine : true,
      required: f.required,
      defaultValue: f.defaultValue,
      ...kindOf(f),
    })),
  };
}

function isDirty(key) {
  const d = drafts[key];
  if (!d) return false;
  const saved = savedOf(key);
  return !saved || JSON.stringify(toSpec(d)) !== JSON.stringify(toSpec(makeDraft(saved)));
}

const draft = computed(() => drafts[selectedKey.value] ?? null);
const dirty = computed(() => isDirty(selectedKey.value));

// Liste de gauche : types enregistrés, puis nouveaux types pas encore enregistrés.
const items = computed(() => [
  ...props.types.map((t) => {
    const key = keyOf(t);
    const changed = isDirty(key);
    return { key, name: drafts[key]?.name.trim() || t.name, summary: typeSummary(changed ? toSpec(drafts[key]) : t), mark: changed ? "modifié" : "" };
  }),
  ...Object.keys(drafts)
    .filter((key) => key.startsWith("n"))
    .map((key) => ({ key, name: drafts[key].name.trim() || "Nouveau type", summary: typeSummary(toSpec(drafts[key])), mark: "nouveau" })),
]);

function select(key) {
  if (!drafts[key]) {
    const saved = savedOf(key);
    if (!saved) return;
    drafts[key] = makeDraft(saved);
  }
  selectedKey.value = key;
  notice.value = null;
  confirmDelete.value = false;
  selCell.value = null;
  selField.value = null;
  pasting.value = null;
}

async function createDraft(t = {}) {
  const key = `n${++newCount}`;
  const linked = t.header == null;
  drafts[key] = makeDraft({ ...t, id: null }, linked);
  if (linked) drafts[key].headerCells = cells([drafts[key].name]);
  select(key);
  await nextTick();
  document.getElementById("type-nom")?.focus();
}

// A1 = le nom du type, tant qu'on ne l'a pas changé.
watch(
  () => draft.value?.name,
  (name) => {
    const d = draft.value;
    if (d?.linked && d.headerCells[0]) d.headerCells[0].text = name.trim();
  },
);

// Nom libre pour un field : « Colonne B », sinon « Colonne B (2) »…
function uniqueName(base, taken) {
  let name = base;
  for (let n = 2; taken.has(tagKey(name)); n++) name = `${base} (${n})`;
  taken.add(tagKey(name));
  return name;
}

// Nouveau type au format du fichier chargé : sa ligne HDR en en-tête, puis un field par colonne
// (numéro du fichier, texte fixe pour une colonne constante, saisie au lot sinon).
function fromFile() {
  const s = props.sheet;
  const taken = new Set();
  const fields = (s?.suggested ?? []).map((text, i) => {
    const col = s.columns[i];
    const base = col?.name || `Colonne ${col?.letter ?? colLetter(i)}`;
    if (text === NUMBER_TAG) return { name: uniqueName(col?.name || "MSISDN", taken), source: "numero" };
    if (!text.startsWith("{{")) return { name: uniqueName(base, taken), source: "formule", formula: text };
    return { name: uniqueName(base, taken), source: "saisie" };
  });
  createDraft({
    description: props.fileName ? `Repris de ${props.fileName}` : "",
    header: s?.batchHeader ?? "",
    fields,
  });
}

const duplicate = () => createDraft({ ...toSpec(draft.value), name: `${draft.value.name.trim()} (copie)` });

function pickFirst() {
  const first = props.types[0];
  if (first) select(keyOf(first));
}

// --- Ligne d'en-tête : texte fixe ou nombre de lignes, cellule par cellule ------------------
const selCell = ref(null); // uid de la cellule choisie
const headerKind = (text) => (/^\{\{([^{}]*)\}\}$/.test(text.trim()) && isTotalTag(text.trim().slice(2, -2)) ? "total" : "texte");

const selected = computed(() => {
  if (selCell.value == null || !draft.value) return null;
  const list = draft.value.headerCells;
  const index = list.findIndex((c) => c.uid === selCell.value);
  if (index < 0) return null;
  const cell = list[index];
  return { index, cell, kind: headerKind(cell.text), ref: `${colLetter(index)}1` };
});

async function selectCell(cellUid) {
  selCell.value = cellUid;
  pasting.value = null;
  await nextTick();
  document.getElementById("cellule-valeur")?.focus();
}

function addCell() {
  const cell = { uid: uid++, text: "" };
  draft.value.headerCells.push(cell);
  selectCell(cell.uid);
}

function moveCell(delta) {
  const s = selected.value;
  const list = draft.value.headerCells;
  const target = s.index + delta;
  if (target < 0 || target >= list.length) return;
  list.splice(target, 0, ...list.splice(s.index, 1));
  draft.value.linked = false;
}

function removeCell() {
  const s = selected.value;
  const list = draft.value.headerCells;
  list.splice(s.index, 1);
  if (s.index === 0) draft.value.linked = false;
  const next = list[Math.min(s.index, list.length - 1)];
  selCell.value = next?.uid ?? null;
}

function setCellKind(kind) {
  const s = selected.value;
  if (!s || s.kind === kind) return;
  s.cell.text = kind === "total" ? "{{total}}" : "";
  if (s.index === 0) draft.value.linked = false;
  if (kind === "texte") nextTick(() => document.getElementById("cellule-valeur")?.focus());
}

// Texte de A1 changé à la main : il ne suit plus le nom.
function onCellText() {
  if (selected.value?.index === 0) draft.value.linked = false;
}

const cellFace = (text) => (headerKind(text) === "total" ? { kind: "total", label: "nombre de lignes" } : { kind: "texte", label: text === "" ? "(vide)" : text });

// Saisir l'en-tête d'un coup.
const pasting = ref(null); // "header" | "fields"
const pasteText = ref("");

async function togglePaste(what) {
  pasting.value = pasting.value === what ? null : what;
  if (!pasting.value) return;
  selCell.value = null;
  pasteText.value = what === "header" ? headerOf(draft.value) : "";
  await nextTick();
  document.getElementById(`coller-${what}`)?.select();
}

function applyHeaderPaste() {
  const text = pasteText.value.replace(/\r?\n$/, "");
  draft.value.headerCells = cells(text.trim() ? splitRaw(text) : []);
  draft.value.linked = false;
  pasting.value = null;
}

// --- Fields -----------------------------------------------------------------------------
const selField = ref(null); // uid du field affiché dans le volet
const fields = computed(() => draft.value?.fields ?? []);

// Lettre de colonne de chaque field écrit dans la ligne (« — » sinon).
const letters = computed(() => {
  let n = 0;
  return fields.value.map((f) => (f.source !== "saisie" || f.inLine ? colLetter(n++) : "—"));
});

const current = computed(() => {
  const i = fields.value.findIndex((f) => f.uid === selField.value);
  return i < 0 ? null : { index: i, field: fields.value[i], ...kindOf(fields.value[i]) };
});

function pickField(f) {
  selField.value = f.uid;
  selCell.value = null;
}

function freshFieldName() {
  const used = new Set(fields.value.map((f) => tagKey(f.name)));
  let n = 1;
  while (used.has(`field${n}`)) n++;
  return `field${n}`;
}

async function addField() {
  const f = blankField({ name: freshFieldName() });
  draft.value.fields.push(f);
  pickField(f);
  await nextTick();
  const input = document.getElementById(`field-nom-${f.uid}`);
  input?.focus();
  input?.select();
}

function moveField(delta) {
  const c = current.value;
  const list = draft.value.fields;
  const target = c.index + delta;
  if (target < 0 || target >= list.length) return;
  list.splice(target, 0, ...list.splice(c.index, 1));
}

function removeField() {
  const c = current.value;
  const list = draft.value.fields;
  list.splice(c.index, 1);
  selField.value = list[Math.min(c.index, list.length - 1)]?.uid ?? null;
}

// Nom sans accolades (elles servent aux formules).
const setName = (f, value) => (f.name = value.replace(/[{}]/g, ""));

// Changer d'où vient la valeur : un numéro ou une formule est toujours écrit dans la ligne.
function setSource(f, source) {
  f.source = source;
  if (source !== "saisie") f.inLine = true;
}

// Coller un tableau : un field par ligne (Field ⇥ Description ⇥ Obligatoire). Un nom déjà présent
// reçoit la description et le caractère obligatoire collés.
const pasted = computed(() => (pasting.value === "fields" ? parseFieldTable(pasteText.value) : []));

function applyFieldPaste() {
  for (const row of pasted.value) {
    const same = draft.value.fields.find((f) => tagKey(f.name) === tagKey(row.name));
    if (same) Object.assign(same, { description: row.description || same.description, required: row.required });
    else draft.value.fields.push(blankField(row));
  }
  pasting.value = null;
}

// Exemple de formule qui se sert d'un field hors ligne.
const formulaHint = (name) => `CRPLMT_${NUMBER_TAG}@{{${name.trim() || "montant"}}}`;

// Formule : insérer le numéro ou un field saisi d'un clic.
const insertable = computed(() => fields.value.filter((f) => f.source === "saisie" && f.name.trim()).map((f) => f.name.trim()));

function insertTag(tag) {
  const input = document.getElementById("field-formule");
  const f = current.value.field;
  const start = input?.selectionStart ?? f.formula.length;
  const end = input?.selectionEnd ?? start;
  f.formula = f.formula.slice(0, start) + tag + f.formula.slice(end);
  nextTick(() => {
    input?.focus();
    input?.setSelectionRange(start + tag.length, start + tag.length);
  });
}

// Type de la valeur choisi à la main : il ne suit plus la devinette.
const kindMenu = ref(null); // { uid, anchor }
const kindMenuField = computed(() => {
  const f = kindMenu.value && fields.value.find((g) => g.uid === kindMenu.value.uid);
  return f ? { name: f.name, ...kindOf(f) } : null;
});
function pickKind(kind) {
  const f = fields.value.find((g) => g.uid === kindMenu.value.uid);
  if (f) Object.assign(f, { kind, guessed: false });
  kindMenu.value = null;
}

// --- Modèle du fichier, construit par Rust sans fichier : ‹numéro›, ‹nametype›… ---------------
const model = ref(null);
const modelError = ref("");
let modelTimer = 0;
let modelSeq = 0;

async function refreshModel() {
  const seq = ++modelSeq;
  const d = draft.value;
  if (!d) return;
  const spec = toSpec(d);
  try {
    const result = await invoke("build_preview", {
      header: spec.header,
      includeHeader: true,
      // Valeur proposée si le type en donne une, sinon le repère du field.
      fields: spec.fields.map((f) => ({ ...f, value: f.source === "saisie" ? f.defaultValue : "", required: true })),
      limit: 0,
      model: true,
    });
    if (seq === modelSeq) {
      model.value = result;
      modelError.value = "";
    }
  } catch (e) {
    if (seq === modelSeq) modelError.value = String(e);
  }
}

watch(
  () => draft.value && JSON.stringify(toSpec(draft.value)),
  () => {
    clearTimeout(modelTimer);
    modelTimer = setTimeout(refreshModel, PREVIEW_DELAY);
  },
  { immediate: true },
);

const segClass = (seg) => ({ xl: seg.kind === "excel", val: seg.kind !== "text" && seg.kind !== "excel" });

// --- Enregistrer, annuler, supprimer ---------------------------------------------
const saving = ref(false);
const notice = ref(null); // { kind: "ok" | "error", text }
const confirmDelete = ref(false);
let confirmTimer = 0;

async function save() {
  const d = draft.value;
  if (!d || saving.value || !dirty.value) return;
  const key = selectedKey.value;
  saving.value = true;
  notice.value = null;
  try {
    const result = await invoke("save_type", { spec: toSpec(d) });
    const savedKey = keyOf(result);
    if (savedKey !== key) delete drafts[key];
    drafts[savedKey] = makeDraft(result);
    selectedKey.value = savedKey;
    selCell.value = null;
    selField.value = null;
    emit("changed", { saved: result });
    notice.value = { kind: "ok", text: `Type « ${result.name} » enregistré.` };
  } catch (e) {
    notice.value = { kind: "error", text: String(e) };
  } finally {
    saving.value = false;
  }
}

function revert() {
  const saved = savedOf(selectedKey.value);
  if (saved) drafts[selectedKey.value] = makeDraft(saved);
  notice.value = null;
  selCell.value = null;
  selField.value = null;
}

async function remove() {
  // 1er clic : demande de confirmation (4 s) ; 2e clic : suppression.
  if (!confirmDelete.value) {
    confirmDelete.value = true;
    clearTimeout(confirmTimer);
    confirmTimer = setTimeout(() => (confirmDelete.value = false), 4000);
    return;
  }
  confirmDelete.value = false;
  const key = selectedKey.value;
  const id = draft.value.id;
  try {
    if (id != null) await invoke("delete_type", { id });
    delete drafts[key];
    selectedKey.value = null;
    if (id != null) emit("changed", { deletedId: id });
    else pickFirst();
  } catch (e) {
    notice.value = { kind: "error", text: String(e) };
  }
}

// Demande d'ouverture venue de « Nouveau lot » (onglet, « Gérer les types », « Créer un type »).
watch(
  () => props.request.seq,
  () => {
    const { id, create } = props.request;
    if (create) return createDraft();
    if (id != null && props.types.some((t) => t.id === id)) select(`t${id}`);
    else if (!selectedKey.value) pickFirst();
  },
  { immediate: true },
);

// Liste rechargée : le brouillon d'un type supprimé part avec lui.
watch(
  () => props.types,
  () => {
    for (const key of Object.keys(drafts)) if (key.startsWith("t") && !savedOf(key)) delete drafts[key];
    if (!drafts[selectedKey.value]) selectedKey.value = null;
    if (!selectedKey.value) pickFirst();
  },
);

// Ctrl+S enregistre le type affiché (seulement quand cet onglet est ouvert).
function onKeydown(e) {
  if (e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "s") {
    e.preventDefault();
    save();
  }
}
onActivated(() => window.addEventListener("keydown", onKeydown));
onDeactivated(() => window.removeEventListener("keydown", onKeydown));
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  clearTimeout(modelTimer);
  clearTimeout(confirmTimer);
});

const HEADER_CONTENTS = [
  { id: "texte", label: "Texte fixe" },
  { id: "total", label: "Nombre de lignes" },
];
</script>

<template>
  <main class="types">
    <aside class="nav" aria-labelledby="t-types">
      <h2 id="t-types" class="nav-title caps">Types d'en-tête</h2>
      <ul class="nav-list">
        <li v-for="it in items" :key="it.key">
          <button
            class="nav-item"
            :class="{ on: it.key === selectedKey }"
            type="button"
            :aria-current="it.key === selectedKey ? 'true' : undefined"
            @click="select(it.key)"
          >
            <span class="nav-name">{{ it.name }}<span v-if="it.mark" class="mark">{{ it.mark }}</span></span>
            <span class="nav-meta">{{ it.summary }}</span>
          </button>
        </li>
      </ul>
      <div class="nav-actions">
        <button class="btn btn-secondary" type="button" @click="createDraft()"><AppIcon name="plus" />Nouveau type</button>
        <button
          v-if="sheet"
          class="btn btn-ghost from-file"
          type="button"
          :title="`Nouveau type au format de ${fileName} : sa ligne HDR et un field par colonne`"
          @click="fromFile"
        >
          <AppIcon name="import" />Depuis le fichier chargé
        </button>
      </div>
    </aside>

    <section v-if="draft" class="editor" aria-label="Type d'en-tête affiché">
      <div class="form">
        <div class="pair">
          <label class="field">
            <span class="field-label">Nom du type</span>
            <input id="type-nom" v-model="draft.name" class="input" type="text" spellcheck="false" autocomplete="off" placeholder="ex. Aloys" />
          </label>
          <label class="field">
            <span class="field-label">Description <span class="opt">facultatif</span></span>
            <input v-model="draft.description" class="input" type="text" autocomplete="off" placeholder="À quoi sert ce format" />
          </label>
        </div>

        <!-- Ligne d'en-tête : texte fixe et nombre de lignes, jamais un field -->
        <section class="block" aria-labelledby="t-header">
          <div class="block-head">
            <h3 id="t-header" class="block-title">Ligne d'en-tête <span class="count">{{ draft.headerCells.length }}</span></h3>
            <button class="btn btn-ghost btn-mini" type="button" :aria-expanded="pasting === 'header'" @click="togglePaste('header')">
              <AppIcon name="edit" />Saisir en une ligne
            </button>
          </div>
          <p class="hint">
            La 1re ligne du fichier : du texte fixe et, au besoin, le nombre de lignes. Aucun field n'y est écrit. Sans cellule, le fichier
            commence directement par les lignes.
          </p>

          <div v-if="pasting === 'header'" class="paste">
            <input
              id="coller-header"
              v-model="pasteText"
              class="input mono"
              type="text"
              spellcheck="false"
              autocomplete="off"
              placeholder="Aloys,{{total}}"
              aria-label="Cellules séparées par des virgules"
              @keydown.enter.prevent="applyHeaderPaste"
              @keydown.esc="pasting = null"
            />
            <button class="btn btn-secondary btn-small" type="button" @click="applyHeaderPaste">Découper</button>
          </div>

          <div class="strip" role="listbox" aria-label="Cellules de la ligne d'en-tête">
            <button
              v-for="(c, i) in draft.headerCells"
              :key="c.uid"
              class="cell"
              :class="[`c-${cellFace(c.text).kind}`, { on: selCell === c.uid }]"
              type="button"
              role="option"
              :aria-selected="selCell === c.uid"
              :title="c.text"
              @click="selectCell(c.uid)"
            >
              <span class="cell-ref">{{ colLetter(i) }}1</span>
              <span class="cell-face"><span class="cell-label">{{ cellFace(c.text).label }}</span></span>
            </button>
            <button class="cell add" type="button" title="Ajouter une cellule à l'en-tête" @click="addCell">
              <AppIcon name="plus" /><span class="sr">Ajouter une cellule</span>
            </button>
          </div>

          <div v-if="selected" class="inspector">
            <div class="ins-head">
              <span class="ins-ref">{{ selected.ref }}</span>
              <div class="seg" role="radiogroup" aria-label="Contenu de la cellule">
                <button
                  v-for="opt in HEADER_CONTENTS"
                  :key="opt.id"
                  class="seg-btn"
                  type="button"
                  role="radio"
                  :aria-checked="selected.kind === opt.id"
                  @click="setCellKind(opt.id)"
                >
                  {{ opt.label }}
                </button>
              </div>
              <span class="ins-fill" />
              <button class="icon-btn" type="button" title="Déplacer à gauche" aria-label="Déplacer à gauche" :disabled="selected.index === 0" @click="moveCell(-1)">
                <AppIcon name="back" />
              </button>
              <button
                class="icon-btn"
                type="button"
                title="Déplacer à droite"
                aria-label="Déplacer à droite"
                :disabled="selected.index === draft.headerCells.length - 1"
                @click="moveCell(1)"
              >
                <AppIcon name="arrow" />
              </button>
              <button class="icon-btn danger" type="button" title="Supprimer la cellule" aria-label="Supprimer la cellule" @click="removeCell">
                <AppIcon name="trash" />
              </button>
            </div>
            <label v-if="selected.kind === 'texte'" class="field">
              <span class="field-label">Texte écrit tel quel<template v-if="selected.index === 0 && draft.linked"> · suit le nom du type</template></span>
              <input
                id="cellule-valeur"
                v-model="selected.cell.text"
                class="input mono"
                type="text"
                spellcheck="false"
                autocomplete="off"
                placeholder="(cellule vide)"
                @input="onCellText"
              />
            </label>
            <p v-else class="hint">Le nombre de lignes de données du fichier généré, calculé à la génération.</p>
          </div>
        </section>

        <!-- Fields : ce qui compose chaque ligne, dans l'ordre des colonnes -->
        <section class="block" aria-labelledby="t-fields">
          <div class="block-head">
            <h3 id="t-fields" class="block-title">Fields <span class="count">{{ fields.length }}</span></h3>
            <button class="btn btn-ghost btn-mini" type="button" :aria-expanded="pasting === 'fields'" @click="togglePaste('fields')">
              <AppIcon name="copy" />Coller un tableau
            </button>
          </div>
          <p class="hint">
            Un field par colonne de chaque ligne, dans l'ordre. Sa description aide la personne qui fera le lot : elle la lira au moment de
            remplir.
          </p>

          <div v-if="pasting === 'fields'" class="paste-table">
            <textarea
              id="coller-fields"
              v-model="pasteText"
              class="input mono"
              rows="4"
              spellcheck="false"
              placeholder="Copiez des lignes d'Excel ou de Word : Field ⇥ Description ⇥ Obligatoire (O/N, Oui/Non, M/O…)"
              aria-label="Tableau des fields à coller"
              @keydown.esc="pasting = null"
            />
            <div class="paste-foot">
              <span class="hint">{{ pasted.length ? `${pasted.length} field${pasted.length > 1 ? "s" : ""} reconnu${pasted.length > 1 ? "s" : ""}` : "Aucune ligne reconnue" }}</span>
              <button class="btn btn-secondary btn-small" type="button" :disabled="!pasted.length" @click="applyFieldPaste">Ajouter</button>
            </div>
          </div>

          <table v-if="fields.length" class="ftable">
            <thead>
              <tr>
                <th scope="col" class="col-ref">Col</th>
                <th scope="col">Field</th>
                <th scope="col">Description</th>
                <th scope="col">Rempli par</th>
                <th scope="col" class="c">Obligatoire</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(f, i) in fields" :key="f.uid" :class="{ on: f.uid === selField }" @click="pickField(f)" @focusin="pickField(f)">
                <td class="col-ref" :title="letters[i] === '—' ? 'Pas dans la ligne : sert aux formules' : `Colonne ${letters[i]}`">{{ letters[i] }}</td>
                <th scope="row">
                  <input
                    :id="`field-nom-${f.uid}`"
                    :value="f.name"
                    class="input fname"
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    placeholder="ex. nametype"
                    :aria-label="`Nom du field ${i + 1}`"
                    @input="setName(f, $event.target.value)"
                  />
                </th>
                <td><input v-model="f.description" class="input" type="text" autocomplete="off" placeholder="À quoi sert ce field" :aria-label="`Description de ${f.name}`" /></td>
                <td>
                  <select class="input select" :value="f.source" :aria-label="`${f.name} rempli par`" @change="setSource(f, $event.target.value)">
                    <option v-for="s in SOURCES" :key="s.id" :value="s.id">{{ s.label }}</option>
                  </select>
                </td>
                <td class="c">
                  <input v-if="f.source === 'saisie'" v-model="f.required" type="checkbox" :aria-label="`${f.name} obligatoire`" />
                  <span v-else class="quiet" aria-hidden="true">—</span>
                </td>
              </tr>
            </tbody>
          </table>
          <p v-else class="hint">Aucun field : ajoutez-en un, ou collez le tableau de la documentation.</p>

          <div class="field-actions">
            <button class="btn btn-secondary btn-small" type="button" @click="addField"><AppIcon name="plus" />Ajouter un field</button>
          </div>

          <!-- Volet du field choisi -->
          <div v-if="current" class="inspector">
            <div class="ins-head">
              <span class="ins-ref">{{ letters[current.index] }}</span>
              <strong class="ins-name">{{ current.field.name || "(sans nom)" }}</strong>
              <span class="ins-fill" />
              <button class="icon-btn" type="button" title="Monter" aria-label="Monter le field" :disabled="current.index === 0" @click="moveField(-1)">
                <AppIcon name="back" class="up" />
              </button>
              <button class="icon-btn" type="button" title="Descendre" aria-label="Descendre le field" :disabled="current.index === fields.length - 1" @click="moveField(1)">
                <AppIcon name="arrow" class="down" />
              </button>
              <button class="icon-btn danger" type="button" title="Supprimer le field" aria-label="Supprimer le field" @click="removeField">
                <AppIcon name="trash" />
              </button>
            </div>

            <label class="field">
              <span class="field-label">Description complète</span>
              <textarea v-model="current.field.description" class="input desc" rows="3" placeholder="Ce que la personne qui fait le lot doit savoir pour remplir ce field" />
            </label>

            <div v-if="current.field.source === 'saisie'" class="ins-grid">
              <div class="field">
                <span class="field-label">Type de la valeur</span>
                <button class="fkind" type="button" @click="kindMenu = { uid: current.field.uid, anchor: $event.currentTarget }">
                  <KindGlyph :kind="current.kind" />
                  <span class="fkind-name">{{ kindLabel(current.kind) }}</span>
                  <span v-if="current.guessed" class="fkind-guess">deviné</span>
                </button>
              </div>
              <label class="field">
                <span class="field-label">Valeur proposée</span>
                <input v-model="current.field.defaultValue" class="input" type="text" spellcheck="false" autocomplete="off" placeholder="aucune" />
              </label>
              <label class="check">
                <input v-model="current.field.inLine" type="checkbox" />
                <span>Écrit dans la ligne</span>
              </label>
              <label class="check">
                <input v-model="current.field.required" type="checkbox" />
                <span>Obligatoire</span>
              </label>
            </div>
            <p v-if="current.field.source === 'saisie'" class="hint">
              Saisi à chaque lot (étape 3), la même valeur sur chaque ligne.<template v-if="!current.field.inLine">
                Pas écrit dans la ligne : il ne sert qu'aux formules, ex. {{ formulaHint(current.field.name) }}.</template
              >
            </p>

            <p v-else-if="current.field.source === 'numero'" class="hint">
              Le numéro de chaque ligne du fichier chargé (paramètre principal, colonne « msisdn ») : une ligne écrite par numéro.
            </p>

            <template v-else>
              <label class="field">
                <span class="field-label">Texte fixe ou formule</span>
                <input id="field-formule" v-model="current.field.formula" class="input mono" type="text" spellcheck="false" autocomplete="off" placeholder="ex. MSISDN ou CRPLMT_{{numero}}@{{montant}}" />
              </label>
              <div class="chips">
                <span class="field-label">Insérer</span>
                <button class="chip c-numero" type="button" @mousedown.prevent @click="insertTag(NUMBER_TAG)">numéro</button>
                <button v-for="name in insertable" :key="name" class="chip c-field" type="button" @mousedown.prevent @click="insertTag(`{{${name}}}`)">{{ name }}</button>
              </div>
            </template>
          </div>
        </section>
      </div>

      <footer class="foot">
        <div class="model selectable" role="region" aria-label="Modèle du fichier" tabindex="0">
          <p v-if="modelError" class="model-error"><AppIcon name="alert" /><span>{{ modelError }}</span></p>
          <ol v-else-if="model" class="model-rows">
            <li v-if="model.headerParts">
              <span class="tag">1</span>
              <code class="txt"><template v-for="(cell, c) in model.headerParts" :key="c"><template v-if="c">,<wbr /></template><template v-for="(seg, q) in cell" :key="q"><span v-if="seg.kind !== 'text'" :class="segClass(seg)">{{ seg.text }}</span><template v-else>{{ seg.text }}</template></template></template></code>
            </li>
            <li v-if="model.sample">
              <span class="tag">{{ model.headerParts ? 2 : 1 }}</span>
              <code class="txt"><template v-for="(field, p) in model.sample.fields" :key="p"><template v-if="p">,<wbr /></template><template v-for="(seg, q) in field" :key="q"><span v-if="seg.kind !== 'text'" :class="segClass(seg)">{{ seg.text }}</span><template v-else>{{ seg.text }}</template></template></template></code>
            </li>
          </ol>
        </div>

        <p v-if="notice" :key="notice.text" class="notice" :class="notice.kind" role="status">
          <AppIcon :name="notice.kind === 'ok' ? 'check' : 'alert'" /><span>{{ notice.text }}</span>
        </p>

        <div class="actions">
          <button class="btn btn-primary" type="button" :disabled="saving || !dirty" aria-keyshortcuts="Control+S" @click="save">
            <AppIcon name="save" />{{ saving ? "Enregistrement…" : "Enregistrer" }}<kbd>Ctrl+S</kbd>
          </button>
          <button v-if="draft.id != null && dirty" class="btn btn-ghost" type="button" @click="revert">Annuler les modifications</button>
          <button v-else-if="draft.id != null" class="btn btn-secondary" type="button" @click="emit('use', draft.id)">
            Utiliser pour un lot<AppIcon name="arrow" />
          </button>
          <span class="spacer" />
          <button class="icon-btn" type="button" title="Dupliquer ce type" aria-label="Dupliquer ce type" @click="duplicate">
            <AppIcon name="copy" />
          </button>
          <span class="gap" aria-hidden="true" />
          <button
            class="btn btn-small delete"
            :class="{ confirm: confirmDelete }"
            type="button"
            :aria-label="confirmDelete ? 'Confirmer la suppression du type' : 'Supprimer ce type'"
            @click="remove"
          >
            <AppIcon name="trash" />{{ confirmDelete ? "Confirmer" : "Supprimer" }}
          </button>
        </div>
      </footer>

      <KindMenu
        v-if="kindMenu && kindMenuField"
        :anchor="kindMenu.anchor"
        :kind="kindMenuField.kind"
        :guessed="kindMenuField.guessed"
        :title="kindMenuField.name"
        @pick="pickKind"
        @close="kindMenu = null"
      />
    </section>

    <section v-else class="editor-empty">
      <div class="empty-box">
        <p class="empty-title">Aucun type affiché</p>
        <p>Choisissez un type à gauche ou créez-en un : sa ligne d'en-tête et ses fields, chacun avec sa description.</p>
        <button class="btn btn-primary" type="button" @click="createDraft()"><AppIcon name="plus" />Nouveau type</button>
      </div>
    </section>
  </main>
</template>

<style scoped>
/* Liste des types à gauche (volet), éditeur à droite : formulaire qui défile, modèle et actions en pied. */
.types {
  display: grid;
  grid-template-columns: clamp(200px, 22%, 260px) minmax(0, 1fr);
  min-height: 0;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 8px 12px 10px;
  border-right: 1px solid var(--border);
  background: var(--surface);
}

.nav-title {
  padding: 0 6px;
  color: var(--text-3);
}

.nav-list {
  display: grid;
  gap: 1px;
  padding: 0;
  list-style: none;
}

.nav-item {
  display: grid;
  width: 100%;
  padding: 6px 8px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--text);
  text-align: left;
  transition: background-color 80ms ease-out;
}

.nav-item:hover {
  background: var(--tint-50);
}

.nav-item.on {
  background: var(--tint-100);
}

.nav-name {
  display: flex;
  align-items: center;
  gap: 6px;
  overflow: hidden;
  font-weight: 600;
  white-space: nowrap;
}

.nav-meta {
  overflow: hidden;
  color: var(--text-3);
  font-size: 11.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* « modifié » / « nouveau » : écrit en toutes lettres, pas seulement une couleur. */
.mark {
  padding: 0 5px;
  border-radius: 3px;
  background: var(--input-fill);
  color: var(--input-text);
  font-size: 10.5px;
  font-weight: 600;
}

.nav-actions {
  display: grid;
  justify-items: start;
  gap: 2px;
  padding: 4px 4px 0;
}

.from-file {
  height: auto;
  min-height: 28px;
  text-align: left;
  white-space: normal;
}

.editor {
  position: relative;
  display: grid;
  grid-template-rows: minmax(0, 1fr) auto;
  min-width: 0;
  min-height: 0;
}

.form {
  display: grid;
  align-content: start;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 18px;
}

.form > * {
  max-width: 900px;
}

.pair {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.3fr);
  gap: 12px;
  padding-bottom: 12px;
}

.field {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.field-label {
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
}

.opt {
  color: var(--text-3);
  font-weight: 400;
}

.block {
  display: grid;
  gap: 8px;
  padding: 10px 0 12px;
  border-top: 1px solid var(--border);
}

.block-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.block-head .block-title {
  margin-right: auto;
}

.block-title {
  display: flex;
  align-items: baseline;
  gap: 7px;
  color: var(--text);
  font-size: 13.5px;
  font-weight: 600;
}

.count {
  color: var(--text-3);
  font-size: 12px;
}

.hint {
  color: var(--text-2);
  font-size: 12px;
}

.mono {
  font-family: var(--font-mono);
  font-size: 12.5px;
}

.paste {
  display: flex;
  gap: 8px;
}

.paste .btn {
  flex: none;
}

/* Rangée de cellules, comme une ligne de feuille : référence en haut à gauche, contenu dessous. */
.strip {
  display: flex;
  overflow-x: auto;
  border: 1px solid var(--grid);
  border-radius: 4px;
  background: var(--sheet);
}

.cell {
  position: relative;
  display: grid;
  flex: none;
  align-content: start;
  gap: 1px;
  min-width: 96px;
  max-width: 220px;
  padding: 3px 8px 5px;
  border: 0;
  border-right: 1px solid var(--grid);
  background: none;
  color: var(--text);
  text-align: left;
  transition: background-color 80ms ease-out;
}

.cell:hover {
  background: var(--tint-50);
}

.cell.on {
  box-shadow: inset 0 0 0 2px var(--ring);
}

.cell-ref {
  color: var(--text-3);
  font-size: 10px;
  font-weight: 600;
}

.cell-face {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  font-size: 12.5px;
}

.cell-face .icon {
  width: 13px;
  height: 13px;
}

.cell-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: pre;
}

.c-texte .cell-label,
.c-formule .cell-label {
  font-family: var(--font-mono);
  font-size: 12px;
}

.c-field {
  background: var(--input-fill);
  color: var(--input-text);
  font-weight: 600;
}

.c-numero {
  background: var(--file-fill);
  color: var(--file-text);
  font-weight: 600;
}

.c-total {
  color: var(--text-2);
  font-style: italic;
}

.cell.add {
  display: grid;
  place-items: center;
  min-width: 40px;
  color: var(--link);
}

.sr {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
}

/* Inspecteur : posé sous la rangée, relié à la cellule choisie. */
.inspector {
  display: grid;
  gap: 10px;
  padding: 10px 12px 12px;
  border: 1px solid var(--border-strong);
  border-radius: 4px;
  background: var(--surface);
}

.ins-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.ins-ref {
  min-width: 34px;
  padding: 2px 6px;
  border-radius: 3px;
  background: var(--tint-100);
  color: var(--text);
  font-size: 11.5px;
  font-weight: 700;
  text-align: center;
}

.ins-fill {
  flex: 1;
}

/* Choix du contenu : boutons segmentés façon Windows. */
.seg {
  display: inline-flex;
  padding: 2px;
  border: 1px solid var(--border-strong);
  border-radius: 4px;
  background: var(--sheet);
}

.seg-btn {
  height: 24px;
  padding: 0 9px;
  border: 0;
  border-radius: 3px;
  background: none;
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

.seg-btn:hover {
  color: var(--text);
}

.seg-btn[aria-checked="true"] {
  background: var(--tint-100);
  color: var(--text);
}

.ins-grid {
  display: grid;
  grid-template-columns: minmax(120px, 1.2fr) minmax(130px, 1fr) minmax(120px, 1.1fr) auto;
  align-items: end;
  gap: 10px;
}

.check {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  font-size: 12.5px;
  font-weight: 600;
  white-space: nowrap;
}

.chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.chip {
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  background: var(--sheet);
  color: var(--text);
  font-size: 12px;
  font-weight: 600;
}

.chip.c-field,
.chip.c-numero {
  border-color: transparent;
}

.chip:hover {
  border-color: var(--ring);
}

.icon-btn {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: var(--radius);
  background: none;
  color: var(--text-2);
  transition:
    background-color 80ms ease-out,
    border-color 80ms ease-out,
    color 80ms ease-out;
}

.icon-btn:hover:not(:disabled) {
  border-color: var(--border-strong);
  background: var(--tint-50);
  color: var(--text);
}

.icon-btn.danger:hover:not(:disabled) {
  border-color: var(--danger-line);
  color: var(--danger);
}

.icon-btn:disabled {
  opacity: 0.3;
}

/* Type de la valeur d'un champ : pictogramme Power Query + nom, « deviné » tant qu'il n'est pas choisi. */
.fkind {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  min-width: 0;
  height: 28px;
  padding: 0 6px 0 2px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius);
  background: var(--sheet);
  color: var(--text);
  font-size: 12.5px;
  transition: border-color 80ms ease-out;
}

.fkind:hover {
  border-color: var(--ring);
}

.fkind :deep(.kind) {
  color: var(--input-text);
}

.fkind-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fkind-guess {
  margin-left: auto;
  color: var(--input-text);
  font-size: 10.5px;
  font-weight: 600;
}

/* Tableau des champs : une ligne par champ, comme une liste de propriétés. */
.ftable {
  width: 100%;
  border-collapse: collapse;
  font-size: 12.5px;
}

.ftable th,
.ftable td {
  padding: 3px 6px;
  border-bottom: 1px solid var(--grid);
  text-align: left;
}

.ftable thead th {
  color: var(--text-3);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.ftable .c {
  text-align: center;
}

.ftable tbody th {
  width: 20%;
  font-weight: 400;
}

/* Pied : modèle du fichier (construit par Rust), message, actions. */
.foot {
  display: grid;
  gap: 8px;
  padding: 8px 18px 10px;
  border-top: 1px solid var(--border);
  background: var(--ribbon);
}

.model {
  max-height: 110px;
  overflow: auto;
  border: 1px solid var(--grid);
  border-radius: 4px;
  background: var(--sheet);
}

.model:focus-visible {
  outline-offset: -2px;
}

.model-rows {
  padding: 0;
  list-style: none;
}

.model-rows li {
  display: grid;
  grid-template-columns: 34px minmax(0, 1fr);
  align-items: start;
}

.model-rows li + li {
  border-top: 1px solid var(--grid);
}

.model-rows .tag {
  align-self: stretch;
  padding-top: 4px;
  padding-right: 8px;
  background: var(--head);
  color: var(--text-3);
  font-size: 11px;
  font-weight: 600;
  text-align: right;
}

.model-rows .txt {
  padding: 0 10px;
  font: 12px / var(--row) var(--font-mono);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.model-rows li:first-child .txt {
  font-weight: 700;
}

.xl {
  background: var(--file-fill);
  color: var(--file-text);
  font-weight: 700;
}

.val {
  background: var(--input-fill);
  color: var(--input-text);
  font-weight: 600;
}

.model-error {
  display: flex;
  gap: 8px;
  padding: 8px 10px;
  background: var(--danger-50);
  color: var(--danger);
  font-size: 12.5px;
}

.model-error .icon {
  margin-top: 2px;
}

.actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.spacer {
  flex: 1;
}

/* La suppression reste isolée : un vide la sépare de toute autre action. */
.gap {
  width: 18px;
}

.actions kbd {
  margin-left: 4px;
}

.delete {
  color: var(--text-2);
}

.delete:hover {
  background: var(--danger-50);
  color: var(--danger);
}

/* Second clic demandé : l'état se lit au texte « Confirmer », pas seulement à la couleur. */
.delete.confirm {
  border-color: var(--danger-line);
  background: var(--danger-50);
  color: var(--danger);
}

.notice {
  display: flex;
  gap: 8px;
  padding: 6px 10px;
  border: 1px solid;
  border-radius: 4px;
  font-size: 12.5px;
  animation: notice-in 200ms var(--ease-out);
}

.notice .icon {
  margin-top: 1px;
}

.notice.ok {
  border-color: var(--ok-line);
  background: var(--ok-fill);
  color: var(--text);
}

.notice.error {
  border-color: var(--danger-line);
  background: var(--danger-50);
  color: var(--danger);
}

@keyframes notice-in {
  from {
    opacity: 0;
    transform: translateY(3px);
  }
}

.editor-empty {
  display: grid;
  place-items: center;
  padding: 24px;
  background: var(--bg);
}

.empty-box {
  display: grid;
  justify-items: center;
  gap: 8px;
  max-width: 360px;
  padding: 18px 20px;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: var(--surface);
  color: var(--text-2);
  font-size: 12.5px;
  text-align: center;
}

.empty-title {
  color: var(--text);
  font-size: 14px;
  font-weight: 600;
}

/* Coller un tableau des fields (Excel, Word). */
.paste-table {
  display: grid;
  gap: 6px;
}

.paste-table textarea,
.desc {
  height: auto;
  min-height: 64px;
  padding: 6px 8px;
  resize: vertical;
  line-height: 1.45;
}

.paste-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.field-actions {
  display: flex;
  gap: 6px;
}

.ftable tbody tr {
  cursor: default;
}

.ftable tbody tr:hover {
  background: var(--tint-50);
}

.ftable tbody tr.on {
  background: var(--tint-100);
}

.ftable .col-ref {
  width: 34px;
  color: var(--text-3);
  font-size: 11px;
  font-weight: 700;
  text-align: center;
}

.ftable .select {
  width: 100%;
}

.ftable input.fname {
  color: var(--input-text);
  font-weight: 700;
}

.quiet {
  color: var(--text-3);
}

.ins-name {
  overflow: hidden;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.icon.up {
  transform: rotate(90deg);
}

.icon.down {
  transform: rotate(90deg);
}

@media (max-width: 900px) {
  .pair {
    grid-template-columns: minmax(0, 1fr);
  }
  .ins-grid {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }
}
</style>
