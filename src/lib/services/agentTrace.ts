import type { AgentTraceEntry } from '$lib/types/agent';

export async function getAgentTrace(messageId: string): Promise<AgentTraceEntry[]> {
  // Agent trace not yet implemented in server backend
  console.warn('[agentTrace] Not yet implemented in server backend');
  return [];
}
