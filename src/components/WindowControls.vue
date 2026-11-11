<script setup>
// Boutons de fenêtre Windows (réduire, agrandir / restaurer, fermer) : la fenêtre n'a pas de cadre
// système, la barre de titre de l'application en tient lieu. Hors de Tauri (aperçu navigateur),
// les boutons restent visibles mais n'agissent pas.
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const win = inTauri ? getCurrentWindow() : null;
const maximized = ref(false);
let unlisten = null;

async function sync() {
  try {
    maximized.value = (await win?.isMaximized()) ?? false;
  } catch {
    // fenêtre indisponible : l'icône reste « agrandir »
  }
}

const act = (name) => () => win?.[name]?.().catch(() => {});
const minimize = act("minimize");
const toggle = act("toggleMaximize");
const close = act("close");

onMounted(async () => {
  await sync();
  try {
    unlisten = await win?.onResized(sync);
  } catch {
    unlisten = null;
  }
});
onBeforeUnmount(() => unlisten?.());
</script>

<template>
  <div class="caption" role="group" aria-label="Fenêtre">
    <button class="cap" type="button" tabindex="-1" aria-label="Réduire" title="Réduire" @click="minimize">
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" /></svg>
    </button>
    <button class="cap" type="button" tabindex="-1" :aria-label="maximized ? 'Restaurer' : 'Agrandir'" :title="maximized ? 'Restaurer' : 'Agrandir'" @click="toggle">
      <svg v-if="maximized" viewBox="0 0 10 10" aria-hidden="true"><path d="M2.5 2.5V.5h7v7h-2" /><path d="M.5 2.5h7v7h-7z" /></svg>
      <svg v-else viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5h9v9h-9z" /></svg>
    </button>
    <button class="cap close" type="button" tabindex="-1" aria-label="Fermer" title="Fermer" @click="close">
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="m.5.5 9 9M9.5.5l-9 9" /></svg>
    </button>
  </div>
</template>

<style scoped>
/* Dimensions des boutons de légende de Windows 11 : 46 × hauteur de la barre, glyphes de 10 px. */
.caption {
  display: flex;
  align-self: stretch;
}

.cap {
  display: grid;
  place-items: center;
  width: 46px;
  border: 0;
  background: none;
  color: var(--chrome-text);
}

.cap svg {
  width: 10px;
  height: 10px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1;
  shape-rendering: crispEdges;
}

.cap:hover {
  background: var(--chrome-hover);
}

.cap:active {
  background: var(--chrome-press);
}

.close:hover {
  background: #c42b1c;
  color: #fff;
}

.close:active {
  background: #b22a1b;
  color: #fff;
}
</style>
