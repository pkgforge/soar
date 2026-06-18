<script setup lang="ts">
interface NavLink {
  text: string
  link: string
}

interface NavGroup {
  title: string
  links: NavLink[]
}

const groups: NavGroup[] = [
  {
    title: 'Getting Started',
    links: [
      { text: 'Quick Start Guide', link: '/quick-start' },
      { text: 'Installation', link: '/installation' },
      { text: 'Configuration', link: '/configuration' },
    ],
  },
  {
    title: 'Package Management',
    links: [
      { text: 'Install Packages', link: '/install' },
      { text: 'Search & Discover', link: '/search' },
      { text: 'Update Packages', link: '/update' },
      { text: 'Remove Packages', link: '/remove' },
    ],
  },
  {
    title: 'Advanced',
    links: [
      { text: 'Profiles', link: '/profiles' },
      { text: 'Declarative Packages', link: '/declarative' },
      { text: 'CLI Reference', link: '/cli-reference' },
    ],
  },
  {
    title: 'Operations',
    links: [
      { text: 'Health & Diagnostics', link: '/health' },
      { text: 'System Maintenance', link: '/maintenance' },
      { text: 'Release Notes', link: '/releases' },
    ],
  },
]
</script>

<template>
  <section class="quick-nav">
    <div class="quick-nav-container">
      <h2 class="quick-nav-heading">Explore the docs</h2>
      <div class="quick-nav-grid">
        <div v-for="group in groups" :key="group.title" class="quick-nav-card">
          <h3>{{ group.title }}</h3>
          <ul>
            <li v-for="item in group.links" :key="item.link">
              <a :href="item.link">{{ item.text }}</a>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </section>
</template>
