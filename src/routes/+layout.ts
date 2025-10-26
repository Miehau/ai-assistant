// Tauri doesn't have a Node.js server to do proper SSR
// Disable both SSR and prerendering for pure client-side rendering
export const prerender = false;
export const ssr = false;
