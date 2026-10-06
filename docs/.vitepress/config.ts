import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Scriptaro',
  description: 'A general-purpose scripted desktop automation engine, written in Rust.',
  base: '/scriptaro/',
  cleanUrls: true,
  lastUpdated: true,
  // Cargo generates these after VitePress; build-api.mjs checks their existence.
  ignoreDeadLinks: [/^\/api\//],
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/scriptaro/logo.svg' }],
    ['meta', { name: 'theme-color', content: '#b74b27' }],
  ],
  themeConfig: {
    logo: '/logo.svg',
    search: { provider: 'local' },
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Examples', link: '/guide/examples' },
      { text: 'Playground', link: '/playground' },
      { text: 'API', link: '/api-reference' },
      { text: '0.1.0 dev', link: 'https://github.com/Almis90/scriptaro/blob/main/CHANGELOG.md' },
    ],
    sidebar: [
      { text: 'Start here', items: [
        { text: 'Getting started', link: '/guide/getting-started' },
        { text: 'Examples', link: '/guide/examples' },
        { text: 'Interactive playground', link: '/playground' },
      ] },
      { text: 'Use Scriptaro', items: [
        { text: 'Prepare a script', link: '/guide/preparation' },
        { text: 'Script format', link: '/script-format' },
        { text: 'Playback and controls', link: '/guide/playback' },
        { text: 'CLI output and reports', link: '/guide/cli-output' },
        { text: 'Variables and sequences', link: '/guide/reuse' },
        { text: 'Motion and assertions', link: '/guide/motion-and-assertions' },
        { text: 'Launch and readiness', link: '/guide/launch-and-readiness' },
        { text: 'Window layout and screenshots', link: '/guide/layout-and-capture' },
        { text: 'Paste prepared text', link: '/guide/paste-text' },
        { text: 'Typing profiles', link: '/guide/typing-profiles' },
        { text: 'Input boundaries and timing', link: '/guide/input-boundaries' },
        { text: 'Desktop interface', link: '/guide/desktop' },
        { text: 'macOS setup', link: '/guide/macos' },
      ] },
      { text: 'Build with Scriptaro', items: [
        { text: 'Architecture', link: '/architecture' },
        { text: 'Rust API reference', link: '/api-reference' },
        { text: 'Native API choices', link: '/native-apis' },
        { text: 'Contributing and docs', link: '/guide/development' },
      ] },
    ],
    socialLinks: [{ icon: 'github', link: 'https://github.com/Almis90/scriptaro' }],
    editLink: {
      pattern: 'https://github.com/Almis90/scriptaro/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
    footer: { message: 'Released under the MIT License.', copyright: 'Copyright © 2026 Almis90' },
    outline: [2, 3],
  },
})
