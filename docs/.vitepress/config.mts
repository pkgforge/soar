import { defineConfig } from 'vitepress'

const description =
  'Documentation for Soar, a fast, modern, bloat-free package manager for Linux. Install static binaries, AppImages, and portable packages across any distro.'

export default defineConfig({
  lang: 'en-US',
    title: 'Soar',
    titleTemplate: ':title · Soar',
    description,
    cleanUrls: true,
    lastUpdated: true,
    appearance: 'dark',
    sitemap: { hostname: 'https://soar.qaidvoid.dev' },

    head: [
      ['link', { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' }],
      ['meta', { name: 'theme-color', content: '#0d1117' }],
      ['meta', { property: 'og:type', content: 'website' }],
      ['meta', { property: 'og:title', content: 'Soar Documentation' }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: 'https://soar.qaidvoid.dev' }],
      [
        'link',
        { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
      ],
      [
        'link',
        { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' },
      ],
      [
        'link',
        {
          rel: 'stylesheet',
          href: 'https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap',
        },
      ],
    ],

    themeConfig: {
      logo: '/favicon.svg',
      siteTitle: 'Soar',

      nav: [
        { text: 'Quick Start', link: '/quick-start' },
        { text: 'CLI Reference', link: '/cli-reference' },
        {
          text: 'Guide',
          items: [
            { text: 'Installation', link: '/installation' },
            { text: 'Configuration', link: '/configuration' },
            { text: 'Profiles', link: '/profiles' },
            { text: 'Package Management', link: '/package-management' },
          ],
        },
        { text: 'Releases', link: '/releases' },
      ],

      sidebar: [
        {
          text: 'Getting Started',
          collapsed: false,
          items: [
            { text: 'Quick Start', link: '/quick-start' },
            { text: 'Installation', link: '/installation' },
            { text: 'Configuration', link: '/configuration' },
            { text: 'Profiles', link: '/profiles' },
            { text: 'CLI Reference', link: '/cli-reference' },
          ],
        },
        {
          text: 'Package Management',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/package-management' },
            { text: 'Declarative Packages', link: '/declarative' },
            { text: 'Install Packages', link: '/install' },
            { text: 'Remove Packages', link: '/remove' },
            { text: 'Update Packages', link: '/update' },
            { text: 'Search Packages', link: '/search' },
            { text: 'List Packages', link: '/list' },
            { text: 'Use Package', link: '/use' },
            { text: 'Run Package', link: '/run' },
            { text: 'Inspect Packages', link: '/inspection' },
          ],
        },
        {
          text: 'Repositories & Files',
          collapsed: false,
          items: [
            { text: 'Repository Management', link: '/repo' },
            { text: 'Download Files', link: '/download' },
          ],
        },
        {
          text: 'Operations',
          collapsed: false,
          items: [
            { text: 'Health', link: '/health' },
            { text: 'Maintenance', link: '/maintenance' },
          ],
        },
        {
          text: 'Release Notes',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/releases' },
            { text: 'Soar 0.12', link: '/releases/v0.12' },
            { text: 'Soar 0.11', link: '/releases/v0.11' },
            { text: 'Soar 0.10', link: '/releases/v0.10' },
          ],
        },
      ],

      socialLinks: [
        { icon: 'github', link: 'https://github.com/pkgforge/soar' },
        { icon: 'discord', link: 'https://discord.gg/djJUs48Zbu' },
      ],

      editLink: {
        pattern: 'https://github.com/pkgforge/soar/edit/main/docs/:path',
        text: 'Edit this page on GitHub',
      },

      search: {
        provider: 'local',
      },

      outline: { level: [2, 3], label: 'On this page' },

      footer: {
        message: 'Released under the MIT License.',
        copyright: 'Copyright © 2024-present pkgforge',
      },

      docFooter: {
        prev: 'Previous',
        next: 'Next',
      },
    },
})
