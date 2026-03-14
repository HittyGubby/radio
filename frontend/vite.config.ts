import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    svelte()],
  server: {
    proxy: {
      '/audio': {
        target: 'ws://127.0.0.1:23331',
        rewrite: () => '/audio',
        ws: true,
      },
      '/spectro': {
        target: 'ws://127.0.0.1:23331',
        rewrite: () => '/spectro',
        ws: true,
      },
      '/config': {
        target: 'http://127.0.0.1:23331',
        rewrite: () => '/config',
        changeOrigin: true,
      },
      '/info': {
        target: 'http://127.0.0.1:23330',
        rewrite: () => '/status?filter=status,name,singer,albumName,duration,progress,picUrl,lyric,tlyric',
        changeOrigin: true,
      },
      '/status': {
        target: 'http://127.0.0.1:23330',
        rewrite: () => '/subscribe-player-status',
        changeOrigin: true,
      },
    },
  },
})
