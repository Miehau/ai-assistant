import { invoke } from '@tauri-apps/api/tauri';
import type { AgentTraceEntry } from '$lib/types/agent';

export async function getAgentTrace(messageId: string): Promise<AgentTraceEntry[]> {
  return await invoke<AgentTraceEntry[]>('agent_get_trace', { messageId });
}
