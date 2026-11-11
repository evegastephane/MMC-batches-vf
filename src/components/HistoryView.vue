<script setup>
// Historique en journal : les lots rangés par jour, le plus récent ouvert avec ses premières lignes.
// Recherche d'un numéro dans tous les lots, re-téléchargement dans le dossier de sortie,
// suppression confirmée d'un second clic. Le contenu des lots est lu par Rust (invoke).
import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import AppIcon from "./AppIcon.vue";
import { markParts, numberIn, shortPath, splitRaw } from "../csvDisplay.js";

const props = defineProps({
  outputDir: { type: String, default: "" },
  dbInfo: { type: Object, default: null }, // { path, sizeBytes, batchCount, numberCount }
  refreshKey: { type: Number, default: 0 },
});
const emit = defineEmits(["changed", "choose-folder", "new-batch"]);

const PREVIEW_LINES = 6; // en-tête + 5 lignes
const MAX_OPEN_RESULTS = 12; // lots ouverts d'office quand on cherche un numéro

const fmt = new Intl.NumberFormat("fr-FR");
const fmt1 = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const plural = (n, one, many) => (n > 1 ? many : one);

function formatBytes(n) {
  if (n < 1024) return `${fmt.format(n)} o`;
  if (n < 1024 * 1024) return `${fmt1.format(n / 1024)} Ko`;
  return `${fmt1.format(n / 1024 / 1024)} Mo`;
}

// Dates écrites par SQLite en heure locale : « AAAA-MM-JJ HH:MM:SS ».
const timeOf = (value) => value.slice(11, 16);

function dayLabel(key) {
  const [y, m, d] = key.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const diff = Math.round((today - date) / 86_400_000);
  if (diff === 0) return "Aujourd'hui";
  if (diff === 1) return "Hier";
  const options = { weekday: "long", day: "numeric", month: "long" };
  if (y !== today.getFullYear()) options.year = "numeric";
  const label = new Intl.DateTimeFormat("fr-FR", options).format(date);
  return label.charAt(0).toUpperCase() + label.slice(1);
}

// --- Liste et recherche ------------------------------------------------------
const query = ref("");
const cleanQuery = computed(() => query.value.replace(/\s+/g, ""));
const batches = ref([]);
const loading = ref(true);
const listError = ref("");
let searchTimer = 0;
let seq = 0;

// Lots regroupés par jour, dans l'ordre (le plus récent en haut).
const days = computed(() => {
  const groups = [];
  for (const batch of batches.value) {
    const key = batch.createdAt.slice(0, 10);
    let group = groups.at(-1);
    if (!group || group.key !== key) groups.push((group = { key, label: dayLabel(key), batches: [], numbers: 0 }));
    group.batches.push(batch);
    group.numbers += batch.dataLines;
  }
  return groups;
});

async function load() {
  const id = ++seq;
  loading.value = true;
  try {
    const list = await invoke("list_batches", { query: cleanQuery.value });
    if (id !== seq) return;
    batches.value = list;
    listError.value = "";
    // Recherche : on ouvre les lots trouvés ; sinon on garde les lots ouverts, ou le plus récent.
    const ids = list.map((b) => b.id);
    const kept = [...openIds.value].filter((i) => ids.includes(i));
    const next = cleanQuery.value ? ids.slice(0, MAX_OPEN_RESULTS) : kept.length ? kept : ids.slice(0, 1);
    openIds.value = new Set(next);
    next.forEach(fetchLines);
  } catch (e) {
    if (id === seq) listError.value = String(e);
  } finally {
    if (id === seq) loading.value = false;
  }
}

watch(() => props.refreshKey, load);
watch(cleanQuery, () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(load, 250);
});

// --- Lots ouverts et leurs premières lignes --------------------------------
const openIds = ref(new Set());
const details = reactive({}); // id → { key, lines, found, error }

const isOpen = (id) => openIds.value.has(id);

function toggle(batch) {
  const next = new Set(openIds.value);
  if (next.has(batch.id)) next.delete(batch.id);
  else {
    next.add(batch.id);
    fetchLines(batch.id);
  }
  openIds.value = next;
}

async function fetchLines(id) {
  const q = cleanQuery.value;
  const key = `${id}|${q}`;
  if (details[id]?.key === key) return;
  try {
    const result = await invoke("batch_lines", { id, limit: PREVIEW_LINES, query: q });
    details[id] = { key, ...result };
  } catch (e) {
    details[id] = { key, lines: [], found: null, error: String(e) };
  }
}

// Lignes à afficher pour un lot : en-tête, lignes de données, puis la ligne trouvée si elle est plus loin.
// Le numéro de chaque ligne est retrouvé grâce à la ligne modèle, pour le surligner partout où il apparaît.
function rowsOf(batch) {
  const d = details[batch.id];
  if (!d) return [];
  const fieldCount = splitRaw(batch.template).length;
  const rows = d.lines.map((text, i) => {
    const no = i + 1;
    const isHeader = i === 0 && batch.header != null;
    const isFound = d.found?.number === no;
    const segs = isHeader || isFound ? null : splitRaw(text);
    const needle = segs ? numberIn(text, batch.template) : null;
    return { no, text, isHeader, isFound, needle, segs: segs?.length === fieldCount ? segs : null };
  });
  if (d.found && d.found.number > d.lines.length) {
    rows.push({ no: null, gap: true });
    rows.push({ no: d.found.number, text: d.found.text, isFound: true });
  }
  return rows;
}

// --- Actions --------------------------------------------------------------
const notice = ref(null); // { id, kind: "ok" | "error", text }
const confirmId = ref(null);
let confirmTimer = 0;

async function download(batch) {
  notice.value = null;
  try {
    const path = await invoke("export_batch", { id: batch.id, folder: props.outputDir });
    notice.value = { id: batch.id, kind: "ok", text: path };
  } catch (e) {
    notice.value = { id: batch.id, kind: "error", text: String(e) };
  }
}

async function remove(batch) {
  // 1er clic : demande de confirmation (4 s) ; 2e clic : suppression.
  if (confirmId.value !== batch.id) {
    confirmId.value = batch.id;
    clearTimeout(confirmTimer);
    confirmTimer = setTimeout(() => (confirmId.value = null), 4000);
    return;
  }
  confirmId.value = null;
  try {
    await invoke("delete_batch", { id: batch.id });
    if (notice.value?.id === batch.id) notice.value = null;
    delete details[batch.id];
    await load();
    emit("changed");
  } catch (e) {
    notice.value = { id: batch.id, kind: "error", text: String(e) };
  }
}

load();
onBeforeUnmount(() => {
  clearTimeout(searchTimer);
  clearTimeout(confirmTimer);
});
</script>

<template>
  <main class="history" aria-labelledby="t-historique">
    <div class="inner">
      <header class="head">
        <div>
          <h2 id="t-historique" class="page-title">Historique</h2>
          <p v-if="dbInfo" class="summary">
            <strong>{{ fmt.format(dbInfo.batchCount) }}</strong> {{ plural(dbInfo.batchCount, "lot", "lots") }} ·
            <strong>{{ fmt.format(dbInfo.numberCount) }}</strong> {{ plural(dbInfo.numberCount, "numéro envoyé", "numéros envoyés") }} ·
            base de <strong>{{ formatBytes(dbInfo.sizeBytes) }}</strong>
          </p>
        </div>
        <div class="dest">
          <span class="dest-label">Téléchargements vers</span>
          <span class="dest-path" :title="outputDir">{{ outputDir ? shortPath(outputDir) : "aucun dossier choisi" }}</span>
          <button class="btn btn-ghost btn-mini" type="button" @click="$emit('choose-folder')">
            <AppIcon name="folder" />Changer…
          </button>
        </div>
      </header>

      <label class="search">
        <AppIcon name="search" />
        <input
          v-model="query"
          class="input search-input"
          type="search"
          inputmode="numeric"
          spellcheck="false"
          placeholder="Rechercher un numéro dans tous les lots, ex. 237653282055"
          aria-label="Rechercher un numéro dans tous les lots"
        />
        <span v-if="cleanQuery && !loading" class="search-count" aria-live="polite">
          {{ fmt.format(batches.length) }} {{ plural(batches.length, "lot", "lots") }}
        </span>
      </label>

      <p v-if="listError" class="error" role="alert"><AppIcon name="alert" /><span>{{ listError }}</span></p>

      <div v-else-if="!loading && !batches.length" class="empty">
        <template v-if="cleanQuery">
          <p class="empty-title">Aucun lot ne contient « {{ cleanQuery }} »</p>
          <p>Vérifiez le numéro, indicatif 237 compris.</p>
        </template>
        <template v-else>
          <p class="empty-title">Aucun lot pour l'instant</p>
          <p>Chaque CSV généré est conservé ici, contenu complet compris, pour être retrouvé ou re-téléchargé.</p>
          <button class="btn btn-primary" type="button" @click="$emit('new-batch')"><AppIcon name="plus" />Créer un lot</button>
        </template>
      </div>

      <section v-for="day in days" :key="day.key" class="day" :aria-labelledby="`jour-${day.key}`">
        <h3 :id="`jour-${day.key}`" class="day-head">
          <span>{{ day.label }}</span>
          <span class="day-count">
            {{ fmt.format(day.batches.length) }} {{ plural(day.batches.length, "lot", "lots") }} ·
            {{ fmt.format(day.numbers) }} {{ plural(day.numbers, "numéro", "numéros") }}
          </span>
        </h3>

        <ol class="lots">
          <li v-for="b in day.batches" :key="b.id" class="lot" :class="{ open: isOpen(b.id) }">
            <button class="lot-head" type="button" :aria-expanded="isOpen(b.id)" :aria-controls="`lot-${b.id}`" @click="toggle(b)">
              <span class="lot-time">{{ timeOf(b.createdAt) }}</span>
              <span class="lot-name">
                <strong :title="b.fileName">{{ b.fileName }}</strong>
                <span>depuis {{ b.sourceName || "fichier inconnu" }}</span>
              </span>
              <span class="lot-figures">
                <span><strong>{{ fmt.format(b.totalLines) }}</strong> {{ plural(b.totalLines, "ligne", "lignes") }}</span>
                <span class="lot-size">{{ formatBytes(b.sizeBytes) }}</span>
              </span>
              <AppIcon name="down" class="chevron" />
            </button>

            <div v-if="isOpen(b.id)" :id="`lot-${b.id}`" class="lot-body">
              <div class="viewer" role="region" :aria-label="`Premières lignes de ${b.fileName}`" tabindex="0">
                <p v-if="details[b.id]?.error" class="viewer-note error-text">{{ details[b.id].error }}</p>
                <p v-else-if="!details[b.id]" class="viewer-note">Lecture…</p>
                <ol v-else class="rows">
                  <li v-for="(r, k) in rowsOf(b)" :key="k" class="row" :class="{ 'is-header': r.isHeader, 'is-found': r.isFound, gap: r.gap }">
                    <span class="no">{{ r.no ?? "" }}</span>
                    <span v-if="r.gap" class="txt gap-txt">… trouvé plus loin, ligne {{ fmt.format(details[b.id].found.number) }} :</span>
                    <code v-else-if="r.isFound" class="txt"><template v-for="(part, q) in markParts(r.text, cleanQuery)" :key="q"><mark v-if="part.hit" class="hit">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template></code>
                    <code v-else-if="r.segs" class="txt"><template v-for="(s, p) in r.segs" :key="p"><template v-if="p">,<wbr /></template><template v-for="(part, q) in markParts(s, r.needle)" :key="q"><span v-if="part.hit" class="num">{{ part.text }}</span><template v-else>{{ part.text }}</template></template></template></code>
                    <code v-else class="txt">{{ r.text }}</code>
                    <span v-if="r.isHeader" class="tag">En-tête</span>
                  </li>
                </ol>
              </div>

              <div class="facts">
                <dl>
                  <div v-if="b.typeName">
                    <dt>Type</dt>
                    <dd>{{ b.typeName }}</dd>
                  </div>
                  <div>
                    <dt>Format</dt>
                    <dd><code><template v-for="(s, p) in splitRaw(b.template)" :key="p"><template v-if="p">,<wbr /></template>{{ s }}</template></code></dd>
                  </div>
                  <div>
                    <dt>Écrit dans</dt>
                    <dd :title="b.filePath">{{ shortPath(b.filePath) }}</dd>
                  </div>
                </dl>
                <div class="actions">
                  <button class="btn btn-secondary btn-small" type="button" :disabled="!outputDir" @click="download(b)">
                    <AppIcon name="download" />Télécharger
                  </button>
                  <button
                    class="btn btn-small delete"
                    :class="{ confirm: confirmId === b.id }"
                    type="button"
                    :aria-label="confirmId === b.id ? `Confirmer la suppression de ${b.fileName}` : `Supprimer ${b.fileName} de l'historique`"
                    @click="remove(b)"
                  >
                    <AppIcon name="trash" />{{ confirmId === b.id ? "Confirmer" : "Supprimer" }}
                  </button>
                </div>
                <p v-if="notice?.id === b.id" class="notice" :class="notice.kind" role="status">
                  <AppIcon :name="notice.kind === 'ok' ? 'check' : 'alert'" />
                  <span v-if="notice.kind === 'ok'" :title="notice.text">Téléchargé dans <code>{{ shortPath(notice.text) }}</code></span>
                  <span v-else>{{ notice.text }}</span>
                </p>
              </div>
            </div>
          </li>
        </ol>
      </section>

      <p v-if="dbInfo && batches.length" class="db-path">
        Base de l'historique : <code :title="dbInfo.path">{{ dbInfo.path }}</code>
      </p>
    </div>
  </main>
</template>

<style scoped>
.history {
  min-height: 0;
  overflow-y: auto;
  scrollbar-width: thin;
  scrollbar-color: var(--border-strong) transparent;
}

/* Colonne centrée : équilibrée en fenêtre agrandie. */
/* Pleine largeur, comme un volet d'application (pas une colonne de page web). */
.inner {
  display: grid;
  gap: 10px;
  padding: 12px 16px 18px;
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: space-between;
  gap: 8px 24px;
}

.page-title {
  color: var(--text);
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
}

.summary {
  margin-top: 1px;
  color: var(--text-2);
  font-size: 12px;
}

.summary strong {
  color: var(--text);
  font-weight: 600;
}

.dest {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 12.5px;
}

.dest-label {
  flex: none;
  color: var(--text-3);
}

.dest-path {
  overflow: hidden;
  color: var(--link);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.btn-mini {
  flex: none;
  gap: 5px;
  height: 26px;
  padding: 0 8px;
  font-size: 12.5px;
}

.btn-mini .icon {
  width: 14px;
  height: 14px;
}

/* Recherche : la question qu'on se pose le plus, « ce numéro est-il déjà parti ? ». */
.search {
  position: relative;
  color: var(--text-3);
}

.search > .icon {
  position: absolute;
  top: 7px;
  left: 10px;
  pointer-events: none;
}

.search-input {
  height: 30px;
  padding: 0 90px 0 32px;
}

.search-count {
  position: absolute;
  top: 5px;
  right: 10px;
  padding: 0 8px;
  border-radius: 3px;
  background: var(--tint-50);
  color: var(--link);
  font-size: 12.5px;
  font-weight: 600;
  line-height: 20px;
}

.error {
  display: flex;
  gap: 8px;
  padding: 9px 11px;
  border: 1px solid var(--danger-line);
  border-radius: var(--radius);
  background: var(--danger-50);
  color: var(--danger);
  font-size: 13px;
}

.empty {
  display: grid;
  justify-items: center;
  gap: 6px;
  padding: 28px 20px;
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius);
  color: var(--text-2);
  text-align: center;
}

.empty-title {
  color: var(--text);
  font-size: 14px;
  font-weight: 600;
}

.empty .btn {
  margin-top: 8px;
}

/* Intercalaire de jour, collé en haut pendant le défilement. */
.day {
  display: grid;
}

.day-head {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  margin: 0;
  padding: 8px 2px;
  background: var(--bg);
  color: var(--link);
  font-size: 13.5px;
  font-weight: 600;
}

.day-count {
  color: var(--text-3);
  font-size: 12.5px;
  font-weight: 400;
}

.lots {
  padding: 0;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
  list-style: none;
}

.lot + .lot {
  border-top: 1px solid var(--grid);
}

.lot-head {
  display: grid;
  grid-template-columns: 52px minmax(0, 1fr) auto 18px;
  align-items: center;
  gap: 14px;
  width: 100%;
  padding: 7px 12px;
  border: 0;
  background: none;
  color: var(--text);
  text-align: left;
  transition: background-color 120ms ease-out;
}

.lot-head:hover,
.open .lot-head {
  background: var(--tint-50);
}

.lot-head:focus-visible {
  outline-offset: -2px;
}

.lot-time {
  color: var(--text-2);
  font-size: 13px;
  font-weight: 600;
}

.lot-name {
  display: grid;
  min-width: 0;
}

.lot-name strong,
.lot-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.lot-name strong {
  color: var(--link);
  font-weight: 600;
}

.lot-name span,
.lot-size {
  color: var(--text-3);
  font-size: 12.5px;
}

.lot-figures {
  display: grid;
  justify-items: end;
  color: var(--text-2);
  white-space: nowrap;
}

.lot-figures strong {
  color: var(--text);
}

.chevron {
  color: var(--text-3);
  transition: transform 90ms ease-out;
}

.open .chevron {
  transform: rotate(180deg);
}

/* Lot ouvert : ses premières lignes à gauche, les faits et actions à droite. */
.lot-body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 270px;
  gap: 18px;
  padding: 14px 16px 16px 82px;
  background: var(--tint-50);
}

.viewer {
  align-self: start;
  overflow: auto;
  max-height: 340px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: linear-gradient(to right, var(--bg) 44px, var(--border) 44px, var(--border) 45px, var(--surface) 45px);
  scrollbar-width: thin;
  scrollbar-color: var(--border-strong) transparent;
}

.viewer:focus-visible {
  outline-offset: -2px;
}

.viewer-note {
  padding: 10px 12px;
  color: var(--text-3);
  font-size: 12.5px;
}

.error-text {
  color: var(--danger);
}

.rows {
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: 45px minmax(0, 1fr) auto;
  align-items: start;
  line-height: 26px;
}

.row + .row {
  border-top: 1px solid var(--grid);
}

.no {
  padding-right: 8px;
  color: var(--text-3);
  font-size: 11.5px;
  text-align: right;
}

.txt {
  padding: 0 10px 0 24px;
  text-indent: -14px; /* une ligne coupée garde un retrait */
  font: 12.5px / 26px var(--font-mono);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.is-header .txt {
  font-weight: 700;
}

.gap-txt {
  color: var(--text-3);
  font: italic 12.5px / 26px var(--font-ui);
}

.is-found {
  background: var(--file-fill);
}

/* Numéros et numéro trouvé : ce qui vient du fichier, sur jaune pâle ; le numéro cherché en jaune plein. */
.num,
.hit {
  border-radius: 3px;
  background: var(--file-fill);
  box-shadow: 0 0 0 1.5px var(--file-fill);
  color: var(--file-text);
  font-weight: 700;
}

.hit {
  background: var(--accent);
  box-shadow: 0 0 0 1.5px var(--accent);
  color: var(--on-accent);
}

.tag {
  margin: 4px 8px 0 0;
  padding: 0 6px;
  border-radius: 3px;
  background: var(--tint-50);
  color: var(--link);
  font-size: 11px;
  font-weight: 600;
  line-height: 18px;
}

.facts {
  display: grid;
  align-content: start;
  gap: 12px;
  min-width: 0;
}

.facts dl {
  display: grid;
  gap: 8px;
  margin: 0;
}

.facts dt {
  color: var(--text-3);
  font-size: 12px;
}

.facts dd {
  margin: 0;
  overflow: hidden;
  color: var(--text);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.facts dd code {
  color: var(--text-2);
  font: 11.5px/1.5 var(--font-mono);
  white-space: normal;
  overflow-wrap: anywhere;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.btn-small {
  height: 30px;
  padding: 0 10px;
  font-size: 13px;
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
  padding: 7px 10px;
  border: 1px solid;
  border-radius: var(--radius);
  font-size: 12.5px;
  animation: notice-in 220ms cubic-bezier(0.16, 1, 0.3, 1);
}

.notice .icon {
  margin-top: 1px;
}

.notice.ok {
  border-color: var(--tint-100);
  background: var(--surface);
  color: var(--link);
}

.notice.error {
  border-color: var(--danger-line);
  background: var(--danger-50);
  color: var(--danger);
}

.notice code {
  font: 11.5px var(--font-mono);
  overflow-wrap: anywhere;
}

@keyframes notice-in {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
}

.db-path {
  overflow: hidden;
  color: var(--text-3);
  font-size: 12px;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.db-path code {
  font: 11.5px var(--font-mono);
}

/* Fenêtre moyenne : plus de retrait, l'aperçu garde sa largeur. */
@media (max-width: 1040px) {
  .lot-body {
    padding-left: 16px;
  }
}

/* Fenêtre étroite : la fiche du lot passe sous ses lignes. */
@media (max-width: 860px) {
  .lot-body {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
