<script setup>
// Pictogrammes au trait (grille 24, trait 1,75), dessinés pour l'app.
defineProps({ name: { type: String, required: true } });

const PATHS = {
  import: ["M14 3H6.5A1.5 1.5 0 0 0 5 4.5v15A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5V8z", "M14 3v5h5", "M12 11v6", "m9 14 3 3 3-3"],
  up: ["m18 15-6-6-6 6"],
  down: ["m6 9 6 6 6-6"],
  x: ["M17 7 7 17", "M7 7l10 10"],
  plus: ["M12 5v14", "M5 12h14"],
  save: ["M18 21l-6-4-6 4V4.5A1.5 1.5 0 0 1 7.5 3h9A1.5 1.5 0 0 1 18 4.5z"],
  trash: ["M4 7h16", "M9 7V4.5h6V7", "M6.5 7l1 13h9l1-13", "M10 11v5.5", "M14 11v5.5"],
  download: ["M12 4v11", "m7 10 5 5 5-5", "M5 20h14"],
  check: ["M5 12.5 10 17l9-10"],
  folder: ["M3.5 7V5.5A1.5 1.5 0 0 1 5 4h4.2l2 2.5H19a1.5 1.5 0 0 1 1.5 1.5v10a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18z", "M3.5 10h17"],
  search: ["M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13z", "m20 20-4.9-4.9"],
  alert: ["M12 3.5 2.5 20h19z", "M12 10v4.5", "M12 17.5v.01"],
  edit: ["M4 20h4L19 9l-4-4L4 16z", "m13.5 6.5 4 4"],
  copy: ["M9 9h10.5v11.5H9z", "M15 9V3.5H4.5V15H9"],
  arrow: ["M5 12h14", "m13 6 6 6-6 6"],
  back: ["M19 12H5", "m11 18-6-6 6-6"],
  calendar: ["M4.5 6h15v14h-15z", "M4.5 10.5h15", "M8.5 3.5v4", "M15.5 3.5v4"],
  phone: ["M7.5 3h9v18h-9z", "M10.5 17.5h3"],
  sun: [
    "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
    "M12 2.5v2.2",
    "M12 19.3v2.2",
    "M2.5 12h2.2",
    "M19.3 12h2.2",
    "m5.3 5.3 1.5 1.5",
    "m17.2 17.2 1.5 1.5",
    "m5.3 18.7 1.5-1.5",
    "m17.2 6.8 1.5-1.5",
  ],
  moon: ["M19.5 14.5A8 8 0 0 1 9.5 4.5a8 8 0 1 0 10 10z"],
  table: ["M3.5 4.5h17v15h-17z", "M3.5 9.5h17", "M3.5 14.5h17", "M9.5 4.5v15"],
  lock: ["M6.5 10.5h11v9h-11z", "M9 10.5V8a3 3 0 0 1 6 0v2.5"],
  caret: ["m7 10 5 5 5-5"],
};
</script>

<template>
  <svg
    class="icon"
    viewBox="0 0 24 24"
    width="16"
    height="16"
    fill="none"
    stroke="currentColor"
    stroke-width="1.75"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <path v-for="d in PATHS[name]" :key="d" :d="d" />
  </svg>
</template>

<style scoped>
.icon {
  flex: none;
}
</style>
