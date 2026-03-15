import type { Plan, PlanStep } from './types'

export function createPlan(goal: string, steps?: PlanStep[]): Plan {
  return {
    goal,
    steps: steps ?? [],
  }
}

export function addStep(
  plan: Plan,
  step: Omit<PlanStep, 'status'> & { status?: PlanStep['status'] }
): Plan {
  return {
    ...plan,
    steps: [...plan.steps, { status: 'pending', ...step }],
  }
}

export function updateStepStatus(
  plan: Plan,
  stepId: string,
  status: PlanStep['status'],
  result?: string
): Plan {
  return {
    ...plan,
    steps: plan.steps.map((step) =>
      step.id === stepId ? { ...step, status, ...(result !== undefined && { result }) } : step
    ),
  }
}

export function removeStep(plan: Plan, stepId: string): Plan {
  return {
    ...plan,
    steps: plan.steps.filter((step) => step.id !== stepId),
  }
}

export function getStep(plan: Plan, stepId: string): PlanStep | undefined {
  return plan.steps.find((step) => step.id === stepId)
}

export function getNextPendingStep(plan: Plan): PlanStep | undefined {
  return plan.steps.find((step) => step.status === 'pending')
}

export function isComplete(plan: Plan): boolean {
  return (
    plan.steps.length > 0 &&
    plan.steps.every((step) => step.status === 'completed' || step.status === 'skipped')
  )
}

export function hasFailed(plan: Plan): boolean {
  return plan.steps.some((step) => step.status === 'failed')
}

export function completedCount(plan: Plan): number {
  return plan.steps.filter((step) => step.status === 'completed').length
}

export function totalCount(plan: Plan): number {
  return plan.steps.length
}
