import type {
	Tool,
	ToolCall,
	ToolResult,
	ToolExecutionContext,
	ErrorCategory
} from '$lib/types/tools';
import { toolRegistry } from './toolRegistry';

interface RateLimitState {
	minuteCalls: { timestamp: number; count: number };
	hourCalls: { timestamp: number; count: number };
}

export class ToolExecutor {
	private rateLimits = new Map<string, RateLimitState>();
	private circuitBreakers = new Map<string, { failures: number; openUntil?: number }>();
	private auditLog: any[] = [];

	async execute(
		call: ToolCall,
		context: ToolExecutionContext,
		retryCount = 0
	): Promise<ToolResult> {
		const startTime = Date.now();

		// Get tool
		const tool = toolRegistry.get(call.tool);
		if (!tool) {
			return this.errorResult('validation', `Unknown tool: ${call.tool}`, false);
		}

		// Validate parameters
		const validation = toolRegistry.validateCall(call);
		if (!validation.valid) {
			// Validation errors don't count as retries - immediate feedback
			return this.errorResult('validation', validation.error, false);
		}

		// Check circuit breaker
		const breaker = this.circuitBreakers.get(call.tool);
		if (breaker && breaker.openUntil && Date.now() < breaker.openUntil) {
			return this.errorResult(
				'server_error',
				`Tool ${call.tool} temporarily disabled due to repeated failures`,
				false
			);
		}

		// Check rate limits
		const rateLimitError = this.checkRateLimit(call.tool, tool.definition.security);
		if (rateLimitError) {
			return this.errorResult('rate_limit', rateLimitError, true);
		}

		// Execute with timeout
		try {
			const result = await this.executeWithTimeout(
				tool.execute(call.parameters, context),
				tool.definition.security.timeout
			);

			// Success - reset circuit breaker
			this.circuitBreakers.delete(call.tool);

			// Audit log
			if (tool.definition.security.auditLog) {
				this.logExecution(call, context, result, Date.now() - startTime);
			}

			return result;
		} catch (error: any) {
			const category = this.categorizeError(error);
			const retriable = this.isRetriable(category);

			// Circuit breaker logic
			if (!retriable || retryCount >= 2) {
				this.recordFailure(call.tool);
			}

			// Retry logic
			if (retriable && retryCount < 3) {
				const delay = this.getRetryDelay(retryCount, category);
				await this.sleep(delay);
				return this.execute(call, context, retryCount + 1);
			}

			return this.errorResult(category, error.message, retriable);
		}
	}

	// Parallel execution for batch calls
	async executeBatch(calls: ToolCall[], context: ToolExecutionContext): Promise<ToolResult[]> {
		return Promise.all(calls.map((call) => this.execute(call, context)));
	}

	private async executeWithTimeout<T>(promise: Promise<T>, timeout: number): Promise<T> {
		return Promise.race([
			promise,
			new Promise<T>((_, reject) =>
				setTimeout(() => reject(new Error('Tool execution timeout')), timeout)
			)
		]);
	}

	private checkRateLimit(toolName: string, security: any): string | null {
		const state = this.rateLimits.get(toolName) || {
			minuteCalls: { timestamp: Date.now(), count: 0 },
			hourCalls: { timestamp: Date.now(), count: 0 }
		};

		const now = Date.now();

		// Check minute limit
		if (now - state.minuteCalls.timestamp < 60000) {
			if (state.minuteCalls.count >= security.rateLimit.maxCallsPerMinute) {
				return `Rate limit exceeded: ${security.rateLimit.maxCallsPerMinute}/min`;
			}
			state.minuteCalls.count++;
		} else {
			state.minuteCalls = { timestamp: now, count: 1 };
		}

		// Check hour limit
		if (now - state.hourCalls.timestamp < 3600000) {
			if (state.hourCalls.count >= security.rateLimit.maxCallsPerHour) {
				return `Rate limit exceeded: ${security.rateLimit.maxCallsPerHour}/hour`;
			}
			state.hourCalls.count++;
		} else {
			state.hourCalls = { timestamp: now, count: 1 };
		}

		this.rateLimits.set(toolName, state);
		return null;
	}

	private categorizeError(error: any): ErrorCategory {
		if (error.message?.includes('timeout')) return 'timeout';
		if (error.message?.includes('rate limit')) return 'rate_limit';
		if (error.message?.includes('401') || error.message?.includes('403')) return 'unauthorized';
		if (error.message?.includes('404')) return 'not_found';
		if (error.message?.includes('5')) return 'server_error';
		return 'network';
	}

	private isRetriable(category: ErrorCategory): boolean {
		return ['network', 'rate_limit', 'server_error', 'timeout'].includes(category);
	}

	private getRetryDelay(retryCount: number, category: ErrorCategory): number {
		if (category === 'rate_limit') return 60000; // 1 min for rate limits
		return Math.min(1000 * Math.pow(2, retryCount), 10000); // Exponential backoff, max 10s
	}

	private recordFailure(toolName: string): void {
		const breaker = this.circuitBreakers.get(toolName) || { failures: 0 };
		breaker.failures++;

		// Open circuit after 5 consecutive failures for 5 minutes
		if (breaker.failures >= 5) {
			breaker.openUntil = Date.now() + 300000;
			console.warn(`[ToolExecutor] Circuit breaker opened for ${toolName}`);
		}

		this.circuitBreakers.set(toolName, breaker);
	}

	private errorResult(category: ErrorCategory, message: string, retriable: boolean): ToolResult {
		return {
			success: false,
			error: { category, message, retriable },
			metadata: { duration: 0, cached: false, timestamp: Date.now() }
		};
	}

	private logExecution(call: ToolCall, context: any, result: any, duration: number): void {
		this.auditLog.push({
			timestamp: Date.now(),
			conversationId: context.conversationId,
			userId: context.userId,
			tool: call.tool,
			parameters: call.parameters,
			success: result.success,
			duration
		});
	}

	private sleep(ms: number): Promise<void> {
		return new Promise((resolve) => setTimeout(resolve, ms));
	}

	getAuditLog() {
		return this.auditLog;
	}
}

export const toolExecutor = new ToolExecutor();
