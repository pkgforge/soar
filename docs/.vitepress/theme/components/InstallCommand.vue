<script setup lang="ts">
import { ref } from 'vue'

const command = 'curl -fsSL https://soar.qaidvoid.dev/install.sh | sh'
const copied = ref(false)

async function copy() {
  try {
    await navigator.clipboard.writeText(command)
    copied.value = true
    setTimeout(() => (copied.value = false), 2000)
  } catch {
    /* clipboard unavailable */
  }
}
</script>

<template>
  <div class="install-command">
    <div class="install-command-inner">
      <span class="install-prompt">$</span>
      <code class="install-text">{{ command }}</code>
      <button
        class="install-copy"
        :class="{ copied }"
        type="button"
        :aria-label="copied ? 'Copied' : 'Copy install command'"
        @click="copy"
      >
        {{ copied ? 'Copied' : 'Copy' }}
      </button>
    </div>
    <p class="install-hint">
      Then add <code>~/.local/share/soar/bin</code> to your <code>PATH</code> and run
      <code>soar install neovim</code>.
    </p>
  </div>
</template>
