<script setup>
// Coquille de l'application de bureau. La fenêtre n'a pas de cadre système : une seule barre
// fait office de barre de titre (on la saisit pour déplacer la fenêtre, double-clic pour agrandir)
// et porte le logo, les onglets, le thème et les boutons de fenêtre.
// Onglets : Nouveau lot (trois écrans), Types d'en-tête, Historique ; les deux premiers gardent
// leur état quand on change d'onglet (KeepAlive).
import { onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import AppIcon from "./components/AppIcon.vue";
import BatchView from "./components/BatchView.vue";
import TypesView from "./components/TypesView.vue";
import HistoryView from "./components/HistoryView.vue";
import WindowControls from "./components/WindowControls.vue";
import logoUrl from "./assets/logo-momo.png";

const OUTPUT_KEY = "mmc-batch:dossier";
const THEME_KEY = "mmc-batch:theme";

const fmt = new Intl.NumberFormat("fr-FR");
const view = ref("lot"); // "lot" | "types" | "historique"

function readStored(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(key)) ?? fallback;
  } catch {
    return fallback;
  }
}

function writeStored(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // stockage indisponible : la valeur reste valable pour la session
  }
}

// --- Thème : sombre par défaut, blanc au choix (mémorisé) ----------------------------
const theme = ref(readStored(THEME_KEY, "dark") === "light" ? "light" : "dark");
const applyTheme = () => (document.documentElement.dataset.theme = theme.value);
applyTheme();

function toggleTheme() {
  theme.value = theme.value === "dark" ? "light" : "dark";
  writeStored(THEME_KEY, theme.value);
  applyTheme();
}

// --- Types d'en-tête (partagés par Nouveau lot et l'onglet des types) ---------------
const types = ref([]);
const typesError = ref("");

async function loadTypes() {
  try {
    types.value = await invoke("list_types");
    typesError.value = "";
  } catch (e) {
    typesError.value = String(e);
  }
}

// Ouvre l'onglet des types sur un type (ou sur un nouveau type).
const typesRequest = ref({ id: null, create: false, seq: 0 });
function openTypes(id = null, create = false) {
  typesRequest.value = { id, create, seq: typesRequest.value.seq + 1 };
  view.value = "types";
}

// « Utiliser pour un lot » depuis l'onglet des types : Nouveau lot prend ce type.
const typeRequest = ref({ id: null, seq: 0 });
function useType(id) {
  typeRequest.value = { id, seq: typeRequest.value.seq + 1 };
  view.value = "lot";
}

// Fichier chargé dans Nouveau lot : l'onglet des types peut en reprendre le format.
const source = ref({ sheet: null, fileName: "" });

// --- Dossier de sortie (mémorisé) et historique ------------------------------------
const outputDir = ref(readStored(OUTPUT_KEY, ""));
const dbInfo = ref(null); // { path, sizeBytes, batchCount, numberCount }
const historyKey = ref(0);

async function chooseFolder() {
  const dir = await open({ directory: true, defaultPath: outputDir.value || undefined, title: "Dossier de sortie des CSV" });
  if (!dir) return;
  outputDir.value = dir;
  writeStored(OUTPUT_KEY, dir);
}

async function refreshDbInfo() {
  try {
    dbInfo.value = await invoke("db_info");
  } catch {
    dbInfo.value = null;
  }
}

function onGenerated() {
  historyKey.value++;
  refreshDbInfo();
}

// --- Comportements de navigateur qui n'ont rien à faire dans une application de bureau ---------
// Menu contextuel du navigateur (sauf dans un champ : couper, copier, coller) ;
// recharger, imprimer, chercher dans la page, code source.
function onContextMenu(e) {
  if (!e.target.closest?.("input, textarea, .selectable")) e.preventDefault();
}

function onBrowserKeys(e) {
  const k = e.key.toLowerCase();
  const reload = k === "f5" || (e.ctrlKey && k === "r");
  const browser = e.ctrlKey && ["p", "f", "g", "j", "u", "h"].includes(k);
  if (import.meta.env.PROD && (reload || browser || k === "f3" || k === "f7")) e.preventDefault();
}

onMounted(async () => {
  document.addEventListener("contextmenu", onContextMenu);
  window.addEventListener("keydown", onBrowserKeys);
  refreshDbInfo();
  loadTypes();
  if (!outputDir.value) outputDir.value = await invoke("default_output_dir").catch(() => "");
});
onBeforeUnmount(() => {
  document.removeEventListener("contextmenu", onContextMenu);
  window.removeEventListener("keydown", onBrowserKeys);
});
</script>

<template>
  <div class="app">
    <header class="titlebar" data-tauri-drag-region>
      <img class="logo" :src="logoUrl" alt="MoMo" width="22" height="22" data-tauri-drag-region />
      <span class="app-name" data-tauri-drag-region>MMC Batch</span>
      <nav class="tabs" role="tablist" aria-label="Sections">
        <button class="tab" role="tab" type="button" :aria-selected="view === 'lot'" @click="view = 'lot'">Nouveau lot</button>
        <button class="tab" role="tab" type="button" :aria-selected="view === 'types'" @click="openTypes()">Types d'en-tête</button>
        <button class="tab" role="tab" type="button" :aria-selected="view === 'historique'" @click="view = 'historique'">
          Historique<span v-if="dbInfo" class="tab-count">{{ fmt.format(dbInfo.batchCount) }}</span>
        </button>
      </nav>
      <span class="drag" data-tauri-drag-region />
      <button
        class="theme"
        type="button"
        :aria-label="theme === 'dark' ? 'Passer au thème blanc' : 'Passer au thème sombre'"
        :title="theme === 'dark' ? 'Thème blanc' : 'Thème sombre'"
        @click="toggleTheme"
      >
        <AppIcon :name="theme === 'dark' ? 'sun' : 'moon'" />
      </button>
      <WindowControls />
    </header>

    <KeepAlive>
      <BatchView
        v-if="view === 'lot'"
        :types="types"
        :types-error="typesError"
        :output-dir="outputDir"
        :type-request="typeRequest"
        @source="source = $event"
        @types-changed="loadTypes"
        @generated="onGenerated"
        @choose-folder="chooseFolder"
        @open-types="openTypes"
      />
    </KeepAlive>

    <KeepAlive>
      <TypesView
        v-if="view === 'types'"
        :types="types"
        :sheet="source.sheet"
        :file-name="source.fileName"
        :request="typesRequest"
        @changed="loadTypes"
        @use="useType"
      />
    </KeepAlive>

    <HistoryView
      v-if="view === 'historique'"
      :output-dir="outputDir"
      :db-info="dbInfo"
      :refresh-key="historyKey"
      @changed="refreshDbInfo"
      @choose-folder="chooseFolder"
      @new-batch="view = 'lot'"
    />
  </div>
</template>

<style scoped>
.app {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  height: 100%;
}

/* Barre de titre unique : bleu nuit MoMo (sombre) ou bleu MoMo (blanc), filet jaune du logo dessous. */
.titlebar {
  display: flex;
  align-items: stretch;
  height: 36px;
  border-bottom: 2px solid var(--accent);
  background: var(--titlebar);
  color: var(--chrome-text);
}

/* Logo tel quel : ni arrondi ni recadrage. */
.logo {
  flex: none;
  align-self: center;
  margin: 0 8px 0 10px;
}

.app-name {
  align-self: center;
  margin-right: 14px;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

.drag {
  flex: 1;
}

/* Onglets dans la barre de titre, comme les onglets de fenêtre de Windows 11. */
.tabs {
  display: flex;
  gap: 1px;
}

.tab {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 0 12px;
  border: 0;
  background: none;
  color: var(--chrome-text);
  font-size: 12.5px;
  font-weight: 600;
  white-space: nowrap;
  opacity: 0.72;
  transition:
    opacity 90ms ease-out,
    background-color 90ms ease-out;
}

.tab:hover {
  background: var(--chrome-hover);
  opacity: 1;
}

.tab[aria-selected="true"] {
  opacity: 1;
}

.tab[aria-selected="true"]::after {
  position: absolute;
  right: 10px;
  bottom: 0;
  left: 10px;
  height: 3px;
  border-radius: 3px 3px 0 0;
  background: var(--accent);
  content: "";
}

.tab:focus-visible,
.theme:focus-visible {
  outline: 2px solid var(--chrome-text);
  outline-offset: -3px;
}

.tab-count {
  min-width: 18px;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--chrome-press);
  font-size: 10.5px;
  line-height: 16px;
  text-align: center;
}

.theme {
  display: grid;
  place-items: center;
  width: 40px;
  border: 0;
  background: none;
  color: var(--chrome-text);
}

.theme:hover {
  background: var(--chrome-hover);
}

.theme .icon {
  width: 15px;
  height: 15px;
}
</style>
