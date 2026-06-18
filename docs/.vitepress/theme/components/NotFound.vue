<script setup lang="ts">
import { withBase } from 'vitepress'
</script>

<template>
  <div class="nf">
    <p class="nf-code">404</p>
    <h1 class="nf-title">Page not found</h1>
    <p class="nf-msg">
      This page does not exist. It may have moved or been removed.
    </p>
    <div class="nf-actions">
      <a class="nf-home" :href="withBase('/')">Back to home</a>
      <p class="nf-links">
        Try
        <a :href="withBase('/quick-start')">Quick Start</a> ·
        <a :href="withBase('/cli-reference')">CLI Reference</a> ·
        <a :href="withBase('/installation')">Installation</a>
      </p>
    </div>
  </div>
</template>

<style scoped>
.nf {
  min-height: 70vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 64px 24px;
}

.nf-code {
  margin: 0;
  font-family: var(--vp-font-family-mono);
  font-size: clamp(4rem, 18vw, 8rem);
  font-weight: 700;
  line-height: 1;
  letter-spacing: -0.04em;
  background: linear-gradient(120deg, #58a6ff 20%, #3fb9a2 60%, #d29922 100%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}

.nf-title {
  margin: 16px 0 8px;
  font-size: 1.5rem;
  border: none;
}

.nf-msg {
  margin: 0 0 28px;
  color: var(--vp-c-text-2);
}

.nf-home {
  display: inline-block;
  padding: 9px 22px;
  border-radius: 20px;
  font-weight: 600;
  color: #06101f;
  background: var(--vp-c-brand-3);
  transition: background 0.2s;
}

.nf-home:hover {
  background: var(--vp-c-brand-2);
}

.nf-links {
  margin-top: 20px;
  font-size: 0.9rem;
  color: var(--vp-c-text-3);
}

.nf-links a {
  font-weight: 500;
}
</style>
