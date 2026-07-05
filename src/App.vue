<script setup lang="ts">
import { onMounted, ref } from "vue";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { useI18n } from "vue-i18n";
import ProjectList from "./components/ProjectList.vue";
import PoolList from "./components/PoolList.vue";
import UsageList from "./components/UsageList.vue";
import { setLocale } from "./i18n";

const tab = ref<"projects" | "pools" | "usage">("projects");
const autostart = ref(false);
const { locale } = useI18n();

onMounted(async () => {
  autostart.value = await isEnabled();
});

async function toggleAutostart() {
  if (autostart.value) {
    await disable();
  } else {
    await enable();
  }
  autostart.value = await isEnabled();
}
</script>

<template>
  <header>
    <h1>ai-control</h1>
    <nav>
      <button :class="{ active: tab === 'projects' }" @click="tab = 'projects'">
        {{ $t("app.projects") }}
      </button>
      <button :class="{ active: tab === 'pools' }" @click="tab = 'pools'">
        {{ $t("app.pools") }}
      </button>
      <button :class="{ active: tab === 'usage' }" @click="tab = 'usage'">
        {{ $t("app.usage") }}
      </button>
    </nav>
    <div class="header-right">
      <div class="lang">
        <button :class="{ active: locale === 'de' }" @click="setLocale('de')">DE</button>
        <button :class="{ active: locale === 'en' }" @click="setLocale('en')">EN</button>
      </div>
      <label class="autostart">
        <input type="checkbox" :checked="autostart" @change="toggleAutostart" />
        {{ $t("app.autostart") }}
      </label>
    </div>
  </header>
  <main>
    <ProjectList v-if="tab === 'projects'" />
    <PoolList v-else-if="tab === 'pools'" />
    <UsageList v-else />
  </main>
</template>
