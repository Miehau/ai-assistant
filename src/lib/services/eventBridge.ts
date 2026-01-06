import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AgentEvent } from '$lib/types';

export async function startAgentEventBridge(
  onEvent: (event: AgentEvent) => void
): Promise<UnlistenFn> {
  return listen<AgentEvent>('agent_event', (event) => {
    onEvent(event.payload);
  });
}
