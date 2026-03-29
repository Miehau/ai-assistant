// Tauri event bridge removed — server SSE is used via honoEventBridge.ts
export async function startAgentEventBridge(): Promise<() => void> {
  return () => {};
}
