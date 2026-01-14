export type PhaseKind = string | Record<string, unknown>;

export type AgentPlan = {
  id: string;
  goal: string;
  assumptions: string[];
  steps: AgentPlanStep[];
  revision_count: number;
  created_at: string;
};

export type AgentPlanStep = {
  id: string;
  sequence: number;
  description: string;
  expected_outcome: string;
  action: AgentStepAction;
  status: AgentStepStatus;
  result?: unknown | null;
  approval?: unknown | null;
};

export type AgentStepAction =
  | { ToolCall: { tool: string; args: Record<string, unknown> } }
  | { AskUser: { question: string } }
  | { Think: { prompt: string } }
  | Record<string, unknown>;

export type AgentStepStatus =
  | 'Pending'
  | 'Proposed'
  | 'Approved'
  | 'Executing'
  | 'Completed'
  | 'Failed'
  | 'Skipped';

export function getPhaseLabel(phase: PhaseKind | null): string {
  if (!phase) return 'Idle';
  if (typeof phase === 'string') {
    return phase;
  }
  const keys = Object.keys(phase);
  return keys.length > 0 ? keys[0] : 'Unknown';
}
