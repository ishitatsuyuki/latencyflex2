import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'LatencyFleX 2',
  description: '',
  themeConfig: {
    editLink: {
      pattern: 'https://github.com/ishitatsuyuki/latencyflex2/edit/master/docs/:path',
      text: 'Edit this page on GitHub',
    },
    nav: [
      { text: 'For Players', link: '/shim/building', activeMatch: '/shim/' },
    ],
    sidebar: {
      '/shim/': [
        {
          text: 'Setup',
          items: [
            { text: 'Building', link: '/shim/building' },
            { text: 'Installation', link: '/shim/installing' },
          ],
        },
      ],
    },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/ishitatsuyuki/latencyflex2' },
    ],
  },
})