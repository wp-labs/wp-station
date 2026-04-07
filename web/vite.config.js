import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react-swc';
import path from 'path';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      // 为 web-tree-sitter 提供浏览器兼容的 polyfill
      'fs/promises': path.resolve(__dirname, './src/polyfills/fs-promises.js'),
      'module': path.resolve(__dirname, './src/polyfills/module.js'),
    },
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        secure: false,
        ws: true
      }
    }
  },
  build: {
    // 增加 chunk 大小警告限制
    chunkSizeWarningLimit: 2000,
    // 使用 esbuild 进行压缩
    minify: 'esbuild',
    rollupOptions: {
      output: {
        // 简化代码分割策略，让 Vite 自动处理依赖顺序
        manualChunks: (id) => {
          // 将所有 node_modules 的内容放到 vendor 中
          // 这样可以避免复杂的依赖顺序问题
          if (id.includes('node_modules')) {
            // web-tree-sitter 单独分离（因为它很大且是可选功能）
            if (id.includes('web-tree-sitter')) {
              return 'tree-sitter';
            }
            // 其他所有依赖放在一起，确保加载顺序正确
            return 'vendor';
          }
        },
      },
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
  },
  // 优化依赖预构建
  optimizeDeps: {
    include: ['react', 'react-dom', 'antd', 'web-tree-sitter'],
  },
});
