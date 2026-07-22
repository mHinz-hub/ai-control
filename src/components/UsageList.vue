<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

interface UsageRow {
  pool: string;
  project: string;
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
  costUsd: number;
}

interface PoolGroup {
  pool: string;
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
  costUsd: number;
  projects: UsageRow[];
}

const { locale } = useI18n();
const days = ref(30);
const rows = ref<UsageRow[]>([]);
const loading = ref(false);
const error = ref("");
const open = ref<Set<string>>(new Set());

async function refresh() {
  loading.value = true;
  try {
    rows.value = await invoke<UsageRow[]>("usage_stats", { days: days.value });
    error.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

watch(days, refresh);
onMounted(refresh);

const groups = computed<PoolGroup[]>(() => {
  const byPool = new Map<string, PoolGroup>();
  for (const r of rows.value) {
    let g = byPool.get(r.pool);
    if (!g) {
      g = {
        pool: r.pool,
        inputTokens: 0,
        outputTokens: 0,
        cacheCreationTokens: 0,
        cacheReadTokens: 0,
        costUsd: 0,
        projects: [],
      };
      byPool.set(r.pool, g);
    }
    g.inputTokens += r.inputTokens;
    g.outputTokens += r.outputTokens;
    g.cacheCreationTokens += r.cacheCreationTokens;
    g.cacheReadTokens += r.cacheReadTokens;
    g.costUsd += r.costUsd;
    g.projects.push(r);
  }
  return [...byPool.values()];
});

const totalCost = computed(() => groups.value.reduce((s, g) => s + g.costUsd, 0));

function toggle(pool: string) {
  const next = new Set(open.value);
  if (next.has(pool)) {
    next.delete(pool);
  } else {
    next.add(pool);
  }
  open.value = next;
}

function fmt(n: number): string {
  return new Intl.NumberFormat(locale.value, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(n);
}

function cost(n: number): string {
  return new Intl.NumberFormat(locale.value, {
    style: "currency",
    currency: "USD",
  }).format(n);
}
</script>

<template>
  <div class="toolbar">
    <div class="range">
      <button :class="{ active: days === 7 }" @click="days = 7">
        {{ $t("usage.days7") }}
      </button>
      <button :class="{ active: days === 30 }" @click="days = 30">
        {{ $t("usage.days30") }}
      </button>
    </div>
    <span class="usage-hint">{{ $t("usage.estimateNote") }}</span>
  </div>

  <p v-if="error" class="error">{{ error }}</p>

  <div v-if="groups.length" class="list-scroll">
  <table class="grid">
    <colgroup>
      <col />
      <col class="col-num" />
      <col class="col-num" />
      <col class="col-num" />
      <col class="col-num" />
      <col class="col-cost" />
    </colgroup>
    <thead>
      <tr>
        <th>{{ $t("usage.pool") }}</th>
        <th class="num">{{ $t("usage.input") }}</th>
        <th class="num">{{ $t("usage.output") }}</th>
        <th class="num">{{ $t("usage.cacheWrite") }}</th>
        <th class="num">{{ $t("usage.cacheRead") }}</th>
        <th class="num">{{ $t("usage.cost") }}</th>
      </tr>
    </thead>
    <tbody>
      <template v-for="g in groups" :key="g.pool">
        <tr class="pool-row" @click="toggle(g.pool)">
          <td class="cell-name">
            <span class="caret">{{ open.has(g.pool) ? "▾" : "▸" }}</span>
            <strong>{{ g.pool }}</strong>
          </td>
          <td class="num">{{ fmt(g.inputTokens) }}</td>
          <td class="num">{{ fmt(g.outputTokens) }}</td>
          <td class="num">{{ fmt(g.cacheCreationTokens) }}</td>
          <td class="num">{{ fmt(g.cacheReadTokens) }}</td>
          <td class="num cost">{{ cost(g.costUsd) }}</td>
        </tr>
        <template v-if="open.has(g.pool)">
          <tr v-for="r in g.projects" :key="g.pool + '/' + r.project" class="project-row">
            <td class="cell-name project-name">{{ r.project }}</td>
            <td class="num">{{ fmt(r.inputTokens) }}</td>
            <td class="num">{{ fmt(r.outputTokens) }}</td>
            <td class="num">{{ fmt(r.cacheCreationTokens) }}</td>
            <td class="num">{{ fmt(r.cacheReadTokens) }}</td>
            <td class="num cost">{{ cost(r.costUsd) }}</td>
          </tr>
        </template>
      </template>
    </tbody>
    <tfoot>
      <tr>
        <td colspan="5" class="total-label">{{ $t("usage.total") }}</td>
        <td class="num cost">{{ cost(totalCost) }}</td>
      </tr>
    </tfoot>
  </table>
  </div>
  <p v-else-if="!loading" class="empty">{{ $t("usage.empty") }}</p>
</template>
