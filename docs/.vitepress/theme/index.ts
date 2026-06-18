import { h } from 'vue'
import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import InstallCommand from './components/InstallCommand.vue'
import QuickNav from './components/QuickNav.vue'
import NotFound from './components/NotFound.vue'
import './custom.css'

/**
 * Custom Soar documentation theme.
 *
 * Extends the VitePress default theme with the Nightsky palette and injects
 * an install command under the home hero plus a quick navigation grid after
 * the feature cards.
 */
export default {
  extends: DefaultTheme,
  Layout() {
    return h(DefaultTheme.Layout, null, {
      'home-hero-after': () => h(InstallCommand),
      'home-features-after': () => h(QuickNav),
      'not-found': () => h(NotFound),
    })
  },
} satisfies Theme
