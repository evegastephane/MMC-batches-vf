<script setup>
// Grille façon Power Query : lettres de colonne avec le type de la valeur, numéros de ligne,
// cellules du fichier (jaune pâle), valeurs saisies (bleu), champs manquants (rouge).
// Le texte des cellules vient toujours de Rust ; la grille ne fait qu'afficher et, si `editable`,
// laisse modifier la valeur d'un champ directement dans une de ses cellules (toute la colonne suit).
import { computed, nextTick, ref, watch } from "vue";
import KindGlyph from "./KindGlyph.vue";
import { colLetter } from "../csvDisplay.js";

const props = defineProps({
  // [{ title, description, kind, from: "file" | "field" | "fixed" | "mixed" | "none", param, editable }]
  columns: { type: Array, required: true },
  // [{ id, no, type: "header" | "data" | "sample" | "more", cells: [[{ text, kind, param }]] | [texte], templates, text }]
  rows: { type: Array, required: true },
  values: { type: Array, default: () => [] }, // valeur saisie de chaque champ (par index)
  editable: { type: Boolean, default: false },
  activeParam: { type: Number, default: -1 },
  flash: { type: Object, default: null }, // { param, seq } : valeur qui vient de changer
  label: { type: String, default: "Aperçu" },
});
const emit = defineEmits(["edit", "select", "pick-kind", "hover-param"]);

const dataRows = computed(() => props.rows.filter((r) => r.type !== "more"));
const sel = ref(null); // { r, c } : index dans dataRows, index de colonne
const editing = ref(null); // { r, c, param, original }
const root = ref(null);
let input = null; // champ de saisie de la cellule en cours de modification
const setInput = (el) => {
  input = el;
};

// Une cellule = des morceaux (aperçu Rust) ou un simple texte (fichier source).
const segsOf = (cell) => (Array.isArray(cell) ? cell : cell == null ? [] : [{ text: String(cell), kind: "text" }]);
const paramOf = (cell) => segsOf(cell).find((s) => s.param != null)?.param ?? null;
const cellAt = (r, c) => dataRows.value[r]?.cells[c];

// Cellule entièrement occupée par une valeur : on la teinte en entier, comme un style de cellule Excel.
function cellKind(cell) {
  const segs = segsOf(cell).filter((s) => s.text !== "");
  if (segs.length === 1 && segs[0].kind !== "text") return segs[0].kind;
  return null;
}

const isSel = (r, c) => sel.value?.r === r && sel.value?.c === c;
const canEdit = (r, c) => props.editable && paramOf(cellAt(r, c)) != null;

function select(r, c, focus = true) {
  const rows = dataRows.value.length;
  if (!rows) return;
  const cols = props.columns.length;
  sel.value = { r: Math.max(0, Math.min(rows - 1, r)), c: Math.max(0, Math.min(cols - 1, c)) };
  const row = dataRows.value[sel.value.r];
  emit("select", {
    ref: `${colLetter(sel.value.c)}${row.no}`,
    template: row.templates?.[sel.value.c] ?? null,
    param: paramOf(cellAt(sel.value.r, sel.value.c)),
  });
  if (focus) root.value?.focus({ preventScroll: true });
  nextTick(() => root.value?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest", inline: "nearest" }));
}

async function startEdit(r, c, seed) {
  if (!canEdit(r, c)) return;
  const param = paramOf(cellAt(r, c));
  editing.value = { r, c, param, original: props.values[param] ?? "" };
  if (seed != null) emit("edit", param, seed);
  await nextTick();
  input?.focus();
  if (seed == null) input?.select();
}

function stopEdit(commit = true) {
  const e = editing.value;
  if (!e) return;
  if (!commit) emit("edit", e.param, e.original);
  editing.value = null;
  root.value?.focus({ preventScroll: true });
}

function onInputKey(e) {
  if (e.key === "Enter") {
    e.preventDefault();
    const { r, c } = editing.value;
    stopEdit(true);
    select(r + 1, c); // comme Excel : Entrée descend d'une ligne
  } else if (e.key === "Escape") {
    e.preventDefault();
    stopEdit(false);
  } else if (e.key === "Tab") {
    e.preventDefault();
    const { r, c } = editing.value;
    stopEdit(true);
    select(r, c + (e.shiftKey ? -1 : 1));
  }
}

function onGridKey(e) {
  if (editing.value || !sel.value) return;
  const { r, c } = sel.value;
  const moves = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] };
  if (moves[e.key]) {
    e.preventDefault();
    select(r + moves[e.key][0], c + moves[e.key][1]);
  } else if (e.key === "Tab") {
    e.preventDefault();
    select(r, c + (e.shiftKey ? -1 : 1));
  } else if (e.key === "Enter" || e.key === "F2") {
    e.preventDefault();
    startEdit(r, c);
  } else if ((e.key === "Delete" || e.key === "Backspace") && canEdit(r, c)) {
    e.preventDefault();
    emit("edit", paramOf(cellAt(r, c)), "");
  } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && canEdit(r, c)) {
    e.preventDefault();
    startEdit(r, c, e.key); // taper remplace la valeur, comme dans un tableur
  }
}

const editValue = computed({
  get: () => (editing.value ? props.values[editing.value.param] ?? "" : ""),
  set: (v) => editing.value && emit("edit", editing.value.param, v),
});

// Le jeu de lignes change (autre type, autre fichier) : la sélection repart de zéro.
watch(
  () => props.columns.length + ":" + dataRows.value.length + ":" + dataRows.value[0]?.type,
  () => {
    editing.value = null;
    if (sel.value && (sel.value.r >= dataRows.value.length || sel.value.c >= props.columns.length)) sel.value = null;
  },
);

// Recopie vers le bas : quand une valeur change, ses cellules s'allument l'une après l'autre.
const flashes = (cell) => props.flash != null && paramOf(cell) === props.flash.param;
</script>

<template>
  <div
    ref="root"
    class="grid-wrap"
    role="grid"
    :aria-label="label"
    :aria-rowcount="rows.length + 1"
    :aria-colcount="columns.length + 1"
    tabindex="0"
    @keydown="onGridKey"
  >
    <table class="grid">
      <thead>
        <tr>
          <th class="corner" aria-hidden="true" />
          <th
            v-for="(col, c) in columns"
            :key="c"
            class="col-head"
            :class="[`from-${col.from}`, { 'is-sel': sel?.c === c, lit: col.param != null && col.param === activeParam }]"
            scope="col"
          >
            <span class="col-title">
              <span class="col-letter" aria-hidden="true">{{ colLetter(c) }}</span>
              <button
                v-if="editable && col.param != null"
                class="kind-btn"
                type="button"
                :title="`Type de « ${col.title} » : cliquer pour le changer`"
                :aria-label="`Changer le type de ${col.title}`"
                @click.stop="emit('pick-kind', col.param, $event.currentTarget)"
              >
                <KindGlyph :kind="col.kind" />
              </button>
              <KindGlyph v-else-if="col.kind" :kind="col.kind" />
              <span class="col-name" :title="col.description ? `${col.title} — ${col.description}` : col.title">{{ col.title || " " }}</span>
            </span>
          </th>
        </tr>
      </thead>
      <tbody>
        <template v-for="row in rows" :key="row.id">
          <tr v-if="row.type === 'more'" class="more-row">
            <th class="row-no" aria-hidden="true" />
            <td :colspan="columns.length" class="more">{{ row.text }}</td>
          </tr>
          <tr v-else :class="`is-${row.type}`">
            <th
              class="row-no"
              scope="row"
              :class="{ 'is-sel': sel && dataRows[sel.r]?.id === row.id }"
              :title="row.type === 'header' ? 'Ligne d\'en-tête' : undefined"
            >
              {{ row.no }}
            </th>
            <td
              v-for="(col, c) in columns"
              :key="c"
              role="gridcell"
              :aria-selected="isSel(dataRows.indexOf(row), c) ? 'true' : undefined"
              :class="[
                cellKind(row.cells[c]) && `k-${cellKind(row.cells[c])}`,
                {
                  editable: canEdit(dataRows.indexOf(row), c),
                  lit: activeParam >= 0 && paramOf(row.cells[c]) === activeParam,
                  empty: !segsOf(row.cells[c]).some((s) => s.text),
                },
              ]"
              @mousedown="select(dataRows.indexOf(row), c)"
              @dblclick="startEdit(dataRows.indexOf(row), c)"
              @mouseenter="paramOf(row.cells[c]) != null && emit('hover-param', paramOf(row.cells[c]))"
              @mouseleave="emit('hover-param', -1)"
            >
              <input
                v-if="editing && dataRows[editing.r]?.id === row.id && editing.c === c"
                :ref="setInput"
                v-model="editValue"
                class="cell-input"
                type="text"
                spellcheck="false"
                autocomplete="off"
                :aria-label="`Valeur de ${columns[c].title || colLetter(c)}`"
                @keydown="onInputKey"
                @blur="stopEdit(true)"
              />
              <span v-else class="cell-text"
                ><template v-for="(seg, q) in segsOf(row.cells[c])" :key="q"
                  ><span v-if="seg.kind !== 'text' && !cellKind(row.cells[c])" :class="`s-${seg.kind}`">{{ seg.text }}</span
                  ><template v-else>{{ seg.text }}</template></template
                ></span
              >
              <i
                v-if="flashes(row.cells[c])"
                :key="flash.seq"
                class="flash"
                :style="{ animationDelay: `${dataRows.indexOf(row) * 14}ms` }"
                aria-hidden="true"
              />
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.grid-wrap {
  position: relative;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  background: var(--bg);
}

.grid-wrap:focus-visible {
  outline: none;
}

.grid {
  border-collapse: separate;
  border-spacing: 0;
  background: var(--sheet);
  font-size: 12.5px;
}

th,
td {
  height: var(--row);
  padding: 0 8px;
  border-right: 1px solid var(--grid);
  border-bottom: 1px solid var(--grid);
  white-space: nowrap;
  text-align: left;
}

/* En-têtes collants : lettres en haut, numéros à gauche, coin fixe. */
thead th {
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--head);
}

.corner {
  left: 0;
  z-index: 3;
  width: 46px;
  min-width: 46px;
}

/* En-tête de colonne sur une seule ligne : lettre, type de la valeur, nom. */
.col-head {
  min-width: 96px;
  max-width: 280px;
  padding: 0 8px 0 6px;
  color: var(--text-2);
  font-weight: 600;
  transition: background-color 80ms ease-out;
}

.col-letter {
  flex: none;
  min-width: 14px;
  color: var(--text-3);
  font-size: 10.5px;
  font-weight: 600;
}

.col-title {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  color: var(--text);
}

.col-name {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Colonne qui vient du fichier : le pictogramme garde la teinte du fichier. */
.from-file .col-title :deep(.kind) {
  color: var(--file-text);
}

.from-field .col-title :deep(.kind) {
  color: var(--input-text);
}

.from-fixed .col-name,
.from-none .col-name {
  color: var(--text-3);
  font-weight: 400;
}

.col-head.is-sel,
.row-no.is-sel {
  background: var(--tint-100);
  color: var(--text);
}

.col-head.is-sel {
  box-shadow: inset 0 -2px 0 var(--ring);
}

.row-no.is-sel {
  box-shadow: inset -2px 0 0 var(--ring);
}

.col-head.lit {
  background: var(--input-fill);
}

.kind-btn {
  display: grid;
  place-items: center;
  height: 20px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  background: var(--sheet);
  color: var(--input-text);
  transition:
    border-color 120ms ease-out,
    background-color 120ms ease-out;
}

.kind-btn:hover {
  border-color: var(--ring);
  background: var(--tint-100);
}

.row-no {
  position: sticky;
  left: 0;
  z-index: 1;
  min-width: 46px;
  background: var(--head);
  color: var(--text-3);
  font-size: 11px;
  font-weight: 600;
  text-align: right;
}

/* Ligne d'en-tête : son numéro sur le jaune du filet MoMo, comme une ligne figée. */
.is-header .row-no {
  color: var(--accent);
}

:root[data-theme="light"] .is-header .row-no {
  color: var(--link);
}

td {
  position: relative;
  max-width: 320px;
  overflow: hidden;
  color: var(--text);
  text-overflow: ellipsis;
  cursor: cell;
}

/* Ligne d'en-tête : en gras, séparée des données par un filet appuyé. */
.is-header td,
.is-header .row-no {
  border-bottom-color: var(--border-strong);
}

.is-header td {
  font-weight: 700;
}

.is-sample td {
  color: var(--text-2);
  font-style: italic;
}

/* Styles de cellule : la provenance de toute la cellule. */
td.k-excel {
  background: var(--file-fill);
  color: var(--file-text);
  font-weight: 600;
}

td.k-value {
  background: var(--input-fill);
  color: var(--input-text);
  font-weight: 600;
}

td.k-missing,
td.k-invalid {
  background: var(--bad-fill);
  color: var(--bad-text);
  font-weight: 600;
}

td.k-invalid .cell-text {
  text-decoration: underline wavy var(--bad-text);
  text-underline-offset: 3px;
}

/* Valeurs à l'intérieur d'un texte (CRPLMT_‹numéro›@10000). */
.s-excel,
.s-value,
.s-missing,
.s-invalid {
  font-weight: 600;
}

.s-excel {
  color: var(--file-text);
  background: var(--file-fill);
}

.s-value {
  color: var(--input-text);
  background: var(--input-fill);
}

.s-missing,
.s-invalid {
  color: var(--bad-text);
  background: var(--bad-fill);
}

td.lit {
  box-shadow: inset 0 0 0 999px var(--tint-100);
}

td.editable:hover {
  box-shadow: inset 0 0 0 1px var(--border-strong);
}

td[aria-selected="true"],
td[aria-selected="true"].editable:hover {
  box-shadow: inset 0 0 0 2px var(--ring);
}

.grid-wrap:not(:focus-within) td[aria-selected="true"] {
  box-shadow: inset 0 0 0 2px var(--border-strong);
}

.cell-input {
  position: absolute;
  inset: 0;
  width: 100%;
  padding: 0 7px;
  border: 2px solid var(--ring);
  background: var(--sheet);
  color: var(--text);
  font: inherit;
  font-weight: 600;
  outline: none;
}

.more {
  color: var(--text-3);
  font-style: italic;
  cursor: default;
}

/* Recopie vers le bas : un éclat jaune qui descend la colonne puis s'éteint. */
.flash {
  position: absolute;
  inset: 0;
  background: var(--accent);
  opacity: 0;
  pointer-events: none;
  animation: fill-down 700ms var(--ease-out) both;
}

@keyframes fill-down {
  0% {
    opacity: 0;
  }
  18% {
    opacity: 0.38;
  }
  100% {
    opacity: 0;
  }
}
</style>
