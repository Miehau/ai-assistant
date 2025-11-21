import type { z } from 'zod';

export interface ToolDefinition {
	name: string;
	description: string;
	parameters: z.ZodType<any>; // Zod schema for validation
	security: ToolSecurityConfig;
	examples?: ToolExample[];
}

export interface ToolSecurityConfig {
	urlAllowlist?: string[];
	urlBlocklist?: string[];
	rateLimit: {
		maxCallsPerMinute: number;
		maxCallsPerHour: number;
	};
	requiresAuth?: boolean;
	auditLog: boolean;
	maxResponseSize?: number; // bytes
	timeout: number; // ms
}

export interface ToolExample {
	scenario: string;
	call: ToolCall;
	expectedResult: string;
}

export interface ToolCall {
	tool: string;
	parameters: Record<string, any>;
}

export interface ToolCallBatch {
	tools: ToolCall[]; // Parallel execution
}

export interface ToolResult {
	success: boolean;
	data?: any;
	error?: {
		category: ErrorCategory;
		message: string;
		retriable: boolean;
		details?: any;
	};
	metadata: {
		duration: number;
		cached: boolean;
		timestamp: number;
	};
}

export type ErrorCategory =
	| 'validation'
	| 'network'
	| 'rate_limit'
	| 'unauthorized'
	| 'not_found'
	| 'server_error'
	| 'timeout';

export interface ToolExecutionContext {
	conversationId: string;
	userId?: string;
	toolCallId: string;
	cache?: Map<string, any>;
}

export interface Tool {
	definition: ToolDefinition;
	execute: (parameters: Record<string, any>, context: ToolExecutionContext) => Promise<ToolResult>;
}
