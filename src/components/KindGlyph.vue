<script setup>
// Pictogramme du type d'une valeur, comme dans les en-têtes de colonne de Power Query :
// ABC (texte), 1²3 (nombre), ID (identifiant), calendrier (date), téléphone.
import AppIcon from "./AppIcon.vue";

defineProps({ kind: { type: String, default: "texte" } });
</script>

<template>
  <span class="kind" :data-kind="kind" aria-hidden="true">
    <AppIcon v-if="kind === 'date'" name="calendar" />
    <AppIcon v-else-if="kind === 'telephone'" name="phone" />
    <template v-else-if="kind === 'nombre'">1<sup>2</sup>3</template>
    <template v-else-if="kind === 'identifiant'">ID</template>
    <template v-else>ABC</template>
  </span>
</template>

<style scoped>
.kind {
  display: inline-grid;
  flex: none;
  place-items: center;
  min-width: 24px;
  height: 16px;
  padding: 0 2px;
  color: inherit;
  font: 700 9.5px/1 var(--font-ui);
  letter-spacing: 0.02em;
}

.kind sup {
  font-size: 7px;
  vertical-align: 3px;
}

.kind .icon {
  width: 14px;
  height: 14px;
  stroke-width: 2;
}
</style>
