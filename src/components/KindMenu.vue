<script setup>
// Menu « Type de la valeur » posé sous son bouton (pictogramme d'une colonne ou d'un champ),
// comme le menu de type d'une colonne Power Query. Échap ou un clic ailleurs le ferme.
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import KindGlyph from "./KindGlyph.vue";
import { KINDS } from "../csvDisplay.js";

const props = defineProps({
  anchor: { type: Object, required: true }, // élément qui a ouvert le menu
  kind: { type: String, default: "texte" },
  guessed: { type: Boolean, default: false },
  title: { type: String, default: "" },
});
const emit = defineEmits(["pick", "close"]);

const menu = ref(null);
const place = computed(() => {
  const r = props.anchor.getBoundingClientRect();
  const left = Math.min(r.left, window.innerWidth - 248);
  const below = r.bottom + 4;
  return below + 236 > window.innerHeight ? { left: `${left}px`, bottom: `${window.innerHeight - r.top + 4}px` } : { left: `${left}px`, top: `${below}px` };
});

function onDoc(e) {
  if (!menu.value?.contains(e.target) && !props.anchor.contains(e.target)) emit("close");
}

function onKey(e) {
  if (e.key === "Escape") {
    e.stopPropagation();
    emit("close");
    props.anchor.focus();
  } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const items = [...menu.value.querySelectorAll("[role=menuitemradio]")];
    const i = items.indexOf(document.activeElement);
    items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
  }
}

onMounted(async () => {
  document.addEventListener("mousedown", onDoc, true);
  await nextTick();
  menu.value?.querySelector('[aria-checked="true"]')?.focus();
});
onBeforeUnmount(() => document.removeEventListener("mousedown", onDoc, true));
</script>

<template>
  <Teleport to="body">
    <div ref="menu" class="menu kind-menu" role="menu" :aria-label="`Type de ${title}`" :style="place" @keydown="onKey">
      <p class="menu-head caps">
        Type de la valeur<span v-if="guessed" class="guess"> · deviné</span>
      </p>
      <button
        v-for="k in KINDS"
        :key="k.id"
        class="kind-item"
        type="button"
        role="menuitemradio"
        :aria-checked="k.id === kind ? 'true' : 'false'"
        @click="emit('pick', k.id)"
      >
        <KindGlyph :kind="k.id" />
        <span class="kind-name">{{ k.label }}</span>
        <span class="kind-hint">{{ k.hint }}</span>
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.kind-menu {
  position: fixed;
  width: 240px;
}

.menu-head {
  padding: 5px 8px 6px;
  color: var(--text-3);
}

.guess {
  color: var(--input-text);
}

.kind-item {
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr);
  align-items: center;
  width: 100%;
  padding: 5px 8px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--text);
  text-align: left;
}

.kind-item :deep(.kind) {
  grid-row: span 2;
  color: var(--text-2);
}

.kind-name {
  font-weight: 600;
}

.kind-hint {
  color: var(--text-3);
  font-size: 11.5px;
}

.kind-item:hover,
.kind-item:focus-visible {
  outline: none;
  background: var(--tint-100);
}

.kind-item[aria-checked="true"] {
  background: var(--input-fill);
}

.kind-item[aria-checked="true"] :deep(.kind),
.kind-item[aria-checked="true"] .kind-name {
  color: var(--input-text);
}
</style>
