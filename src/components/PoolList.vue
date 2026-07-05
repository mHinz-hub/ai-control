<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

const { t } = useI18n();

interface Pool {
  id: string;
  name: string;
  credentialType: string;
  projects: string[];
  running: string[];
  hasCredentials: boolean;
}

const pools = ref<Pool[]>([]);
const error = ref("");
const busy = ref(false);

type Mode = "oauth" | "apikey";
const dialog = ref<{ mode: Mode; editing: boolean } | null>(null);
const dName = ref("");
const dKey = ref("");
// Ziel-Pool (ID) beim Key-Ändern; bei Neuanlage leer.
const dPool = ref("");

const dialogTitle = computed(() => {
  if (!dialog.value) return "";
  if (dialog.value.mode === "oauth") return t("pools.newOauthPool");
  if (dialog.value.editing) return t("pools.editKey", { name: dName.value });
  return t("pools.newApikeyPool");
});

function openNew(mode: Mode) {
  dName.value = "";
  dKey.value = "";
  dPool.value = "";
  error.value = "";
  dialog.value = { mode, editing: false };
}

function openEditKey(pool: Pool) {
  dName.value = pool.name;
  dKey.value = "";
  dPool.value = pool.id;
  error.value = "";
  dialog.value = { mode: "apikey", editing: true };
}

function closeDialog() {
  if (busy.value) return;
  dialog.value = null;
}

async function refresh() {
  try {
    pools.value = await invoke<Pool[]>("list_pools");
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}

// Zweistufig: erster Versuch ohne Datei-Erlaubnis; meldet das Backend
// keychain-unavailable, fragt ein Dialog nach und wiederholt mit allowFile.
const fileConfirm = ref(false);

async function submit(allowFile = false) {
  const d = dialog.value;
  if (!d) return;
  busy.value = true;
  error.value = "";
  try {
    if (d.mode === "oauth") {
      await invoke("create_oauth_pool", { name: dName.value });
    } else if (d.editing) {
      await invoke("set_apikey", { pool: dPool.value, key: dKey.value, allowFile });
    } else {
      await invoke("create_apikey_pool", { name: dName.value, key: dKey.value, allowFile });
    }
    dialog.value = null;
    await refresh();
  } catch (e) {
    if (String(e) === "keychain-unavailable") {
      fileConfirm.value = true;
    } else {
      error.value = String(e);
    }
  } finally {
    busy.value = false;
  }
}

async function confirmFileFallback() {
  fileConfirm.value = false;
  await submit(true);
}

const deletePool = ref<Pool | null>(null);

function askDelete(pool: Pool) {
  error.value = "";
  deletePool.value = pool;
}

async function confirmDelete() {
  const pool = deletePool.value;
  if (!pool) return;
  busy.value = true;
  error.value = "";
  try {
    await invoke("delete_pool", { pool: pool.id });
    deletePool.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

const renamePool = ref<Pool | null>(null);
const rName = ref("");

function openRename(pool: Pool) {
  error.value = "";
  rName.value = pool.name;
  renamePool.value = pool;
}

async function confirmRename() {
  const pool = renamePool.value;
  if (!pool) return;
  busy.value = true;
  error.value = "";
  try {
    await invoke("rename_pool", { pool: pool.id, name: rName.value });
    renamePool.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

interface Project {
  name: string;
  pool: string | null;
  running: boolean;
}

const reloginPool = ref<Pool | null>(null);
const reloginHasEntry = ref(false);
const reloginRunning = ref<string[]>([]);

async function askRelogin(pool: Pool) {
  error.value = "";
  try {
    reloginHasEntry.value = await invoke<boolean>("keychain_status", { pool: pool.id });
    const projects = await invoke<Project[]>("list_projects");
    reloginRunning.value = projects
      .filter((p) => p.pool === pool.id && p.running)
      .map((p) => p.name);
    reloginPool.value = pool;
  } catch (e) {
    error.value = String(e);
  }
}

async function confirmRelogin() {
  const pool = reloginPool.value;
  if (!pool) return;
  busy.value = true;
  error.value = "";
  try {
    await invoke("oauth_login", { pool: pool.id });
    reloginPool.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
    reloginPool.value = null;
  } finally {
    busy.value = false;
  }
}

onMounted(refresh);
</script>

<template>
  <div class="toolbar">
    <button class="primary" :disabled="busy" @click="openNew('oauth')">
      {{ $t("pools.newOauth") }}
    </button>
    <button class="primary" :disabled="busy" @click="openNew('apikey')">
      {{ $t("pools.newApikey") }}
    </button>
  </div>

  <p v-if="error" class="error">{{ error }}</p>

  <table v-if="pools.length" class="grid">
    <colgroup>
      <col />
      <col class="col-type" />
      <col class="col-projects" />
      <col class="col-actions-wide" />
    </colgroup>
    <thead>
      <tr>
        <th>{{ $t("pools.pool") }}</th>
        <th>{{ $t("pools.type") }}</th>
        <th></th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="p in pools" :key="p.id">
        <td class="cell-name">
          <strong>{{ p.name }}</strong>
        </td>
        <td>
          <span class="badge" :class="p.credentialType">{{ p.credentialType }}</span>
        </td>
        <td>
          <span v-if="p.projects.length" class="assigned hover-pop">
            {{ $t("pools.assigned", p.projects.length) }}
            <span class="pop">
              <span v-for="name in p.projects" :key="name" class="pop-item">
                {{ name }}
              </span>
            </span>
          </span>
        </td>
        <td class="cell-actions">
          <span class="row-actions">
            <button :disabled="busy" @click="openRename(p)">
              {{ $t("pools.rename") }}
            </button>
            <template v-if="p.credentialType === 'oauth'">
              <button class="w-action" :disabled="busy" @click="askRelogin(p)">
                {{ $t("pools.relogin") }}
              </button>
            </template>
            <template v-else>
              <button class="w-action" :disabled="busy" @click="openEditKey(p)">
                {{ p.hasCredentials ? $t("pools.changeKey") : $t("pools.insertKey") }}
              </button>
            </template>
            <span class="tip-wrap" :class="{ 'hover-pop': p.running.length }">
              <button
                class="danger"
                :disabled="busy || p.running.length > 0"
                @click="askDelete(p)"
              >
                {{ $t("pools.delete") }}
              </button>
              <span v-if="p.running.length" class="pop pop-right">
                <span class="pop-text">
                  {{ $t("pools.deleteBlockedTooltip", { projects: p.running.join(", ") }) }}
                </span>
              </span>
            </span>
          </span>
        </td>
      </tr>
    </tbody>
  </table>
  <p v-else class="empty">{{ $t("pools.empty") }}</p>

  <div v-if="dialog" class="overlay" @click.self="closeDialog">
    <form class="dialog" @submit.prevent="submit()">
      <h3>{{ dialogTitle }}</h3>

      <label v-if="!dialog.editing" class="field">
        {{ $t("pools.name") }}
        <input v-model="dName" placeholder="z. B. private" autofocus required />
      </label>

      <label v-if="dialog.mode === 'apikey'" class="field">
        {{ $t("pools.apiKey") }}
        <input
          v-model="dKey"
          type="password"
          placeholder="sk-ant-api03-…"
          :autofocus="dialog.editing"
          required
        />
      </label>

      <p v-if="dialog.mode === 'oauth'" class="hint">
        {{ $t("pools.oauthHint") }}
      </p>

      <div class="actions">
        <button type="button" :disabled="busy" @click="closeDialog">
          {{ $t("pools.cancel") }}
        </button>
        <button type="submit" class="primary" :disabled="busy">
          {{ dialog.mode === "oauth" ? $t("pools.createPool") : $t("pools.save") }}
        </button>
      </div>
    </form>
  </div>

  <div
    v-if="reloginPool"
    class="overlay"
    @click.self="busy ? undefined : (reloginPool = null)"
  >
    <div class="dialog">
      <h3>{{ $t("pools.reloginTitle", { name: reloginPool.name }) }}</h3>
      <template v-if="reloginRunning.length">
        <p class="hint">{{ $t("pools.reloginBlocked") }}</p>
        <ul class="affected">
          <li v-for="name in reloginRunning" :key="name">{{ name }}</li>
        </ul>
        <div class="actions">
          <button type="button" @click="reloginPool = null">
            {{ $t("pools.cancel") }}
          </button>
        </div>
      </template>
      <template v-else>
        <p class="hint">
          {{
            reloginHasEntry
              ? $t("pools.reloginWarning", { name: reloginPool.name })
              : $t("pools.reloginNoEntry")
          }}
        </p>
        <div class="actions">
          <button type="button" :disabled="busy" @click="reloginPool = null">
            {{ $t("pools.cancel") }}
          </button>
          <button class="primary" :disabled="busy" @click="confirmRelogin">
            {{ $t("pools.reset") }}
          </button>
        </div>
      </template>
    </div>
  </div>

  <div v-if="deletePool" class="overlay" @click.self="deletePool = null">
    <div class="dialog">
      <h3>{{ $t("pools.deletePool") }}</h3>
      <p class="hint">{{ $t("pools.deleteWarning", { name: deletePool.name }) }}</p>
      <template v-if="deletePool.projects.length">
        <p class="hint">{{ $t("pools.deleteUnassigns") }}</p>
        <ul class="affected">
          <li v-for="name in deletePool.projects" :key="name">{{ name }}</li>
        </ul>
      </template>
      <div class="actions">
        <button type="button" :disabled="busy" @click="deletePool = null">
          {{ $t("pools.cancel") }}
        </button>
        <button class="danger" :disabled="busy" @click="confirmDelete">
          {{ $t("pools.delete") }}
        </button>
      </div>
    </div>
  </div>

  <div v-if="fileConfirm" class="overlay">
    <div class="dialog">
      <h3>{{ $t("pools.keychainUnavailableTitle") }}</h3>
      <p class="hint">{{ $t("pools.keychainUnavailable") }}</p>
      <div class="actions">
        <button type="button" :disabled="busy" @click="fileConfirm = false">
          {{ $t("pools.cancel") }}
        </button>
        <button class="danger" :disabled="busy" @click="confirmFileFallback">
          {{ $t("pools.storeAsFile") }}
        </button>
      </div>
    </div>
  </div>

  <div v-if="renamePool" class="overlay" @click.self="busy ? undefined : (renamePool = null)">
    <form class="dialog" @submit.prevent="confirmRename">
      <h3>{{ $t("pools.renameTitle", { name: renamePool.name }) }}</h3>
      <label class="field">
        {{ $t("pools.name") }}
        <input v-model="rName" autofocus required />
      </label>
      <div class="actions">
        <button type="button" :disabled="busy" @click="renamePool = null">
          {{ $t("pools.cancel") }}
        </button>
        <button type="submit" class="primary" :disabled="busy">
          {{ $t("pools.save") }}
        </button>
      </div>
    </form>
  </div>
</template>
