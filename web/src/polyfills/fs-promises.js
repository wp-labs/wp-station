// Polyfill for fs/promises in browser environment
// web-tree-sitter tries to import this but it's not needed in browser
export default {};
export const readFile = () => Promise.reject(new Error('fs/promises not available in browser'));
export const writeFile = () => Promise.reject(new Error('fs/promises not available in browser'));
