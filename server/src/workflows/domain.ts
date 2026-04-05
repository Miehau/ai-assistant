import type { WorkflowRun, WorkflowRunStatus } from './types.js'

function assertStatus(run: WorkflowRun, allowed: WorkflowRunStatus[]): void {
  if (!allowed.includes(run.status)) {
    throw new Error(
      `Invalid workflow run transition: current status '${run.status}' not in [${allowed.join(', ')}]`
    )
  }
}

export function startRun(run: WorkflowRun): WorkflowRun {
  assertStatus(run, ['pending'])
  const now = Date.now()
  return { ...run, status: 'running', startedAt: now, updatedAt: now }
}

export function completeRun(run: WorkflowRun, output: unknown): WorkflowRun {
  assertStatus(run, ['running'])
  const now = Date.now()
  return { ...run, status: 'completed', output, completedAt: now, updatedAt: now }
}

export function failRun(run: WorkflowRun, error: string): WorkflowRun {
  assertStatus(run, ['pending', 'running'])
  const now = Date.now()
  return { ...run, status: 'failed', error, completedAt: now, updatedAt: now }
}

export function cancelRun(run: WorkflowRun): WorkflowRun {
  assertStatus(run, ['pending', 'running'])
  const now = Date.now()
  return { ...run, status: 'cancelled', completedAt: now, updatedAt: now }
}
