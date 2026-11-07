<script setup lang="ts">
/** Minimal feather-style inline icon set. */
const props = defineProps<{ name: string; size?: number }>();

const paths: Record<string, string[]> = {
  home:["m3 10 9-7 9 7v11H3Z","M9 21v-8h6v8"],
  arrow:["M5 12h14m-6-6 6 6-6 6"],
  window:["M3 4h18v15H3Z","M3 8h18M9 15h6"],
  power:["M12 2v10","M6.3 5.3a9 9 0 1 0 11.4 0"],
  star:["m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.3 6.4 20.2l1.1-6.2L3 9.6l6.2-.9Z"],
  folder:["M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"],
  channel:["M4 4h16v12H9l-5 4Z","M8 8h8M8 12h5"],
  video:["M3 5h18v14H3Z","m10 9 5 3-5 3Z"],
  image:["M3 3h18v18H3Z","m3 16 5-5 4 4 3-3 6 6","M15 7h.01"],
  audio:["M9 18V5l12-2v13","M9 18a3 3 0 1 1-3-3h3","M21 16a3 3 0 1 1-3-3h3"],
  plus:["M12 5v14M5 12h14"],
  close:["m6 6 12 12M6 18 18 6"],
  edit:["m16 3 5 5-12 12-6 1 1-6Z","m14 5 5 5"],
  trash:["M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"],
  download: ["M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", "M7 10l5 5 5-5", "M12 15V3"],
  watch: [
    "M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z",
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  ],
  history: ["M3 11a9 9 0 1 1 2.6 6.4", "M3 4v7h7", "M12 7v5l3 2"],
  settings: [
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
    "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z",
  ],
  file: ["M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z", "M13 2v7h7"],
};
</script>

<template>
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    :width="props.size ?? 17"
    :height="props.size ?? 17"
  >
    <path v-for="(d, i) in paths[props.name] ?? []" :key="i" :d="d" />
  </svg>
</template>
