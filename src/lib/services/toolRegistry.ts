import type { Tool, ToolDefinition, ToolCall } from '$lib/types/tools';
import { validateToolParameters } from './validation';
import { zodToJsonSchema } from 'zod-to-json-schema';

class ToolRegistry {
	private tools = new Map<string, Tool>();

	register(tool: Tool): void {
		if (this.tools.has(tool.definition.name)) {
			throw new Error(`Tool ${tool.definition.name} already registered`);
		}
		this.tools.set(tool.definition.name, tool);
		console.log(`[ToolRegistry] Registered tool: ${tool.definition.name}`);
	}

	get(name: string): Tool | undefined {
		return this.tools.get(name);
	}

	getAll(): Tool[] {
		return Array.from(this.tools.values());
	}

	getAllDefinitions(): ToolDefinition[] {
		return this.getAll().map((t) => t.definition);
	}

	validateCall(call: ToolCall): { valid: true } | { valid: false; error: string } {
		const tool = this.get(call.tool);
		if (!tool) {
			return { valid: false, error: `Unknown tool: ${call.tool}` };
		}

		const validation = validateToolParameters(tool.definition.parameters, call.parameters);

		if (!validation.success) {
			return { valid: false, error: validation.error };
		}

		return { valid: true };
	}

	// Generate JSON schema for agent LLM system prompt
	toJSONSchema(): any[] {
		return this.getAll().map((tool) => ({
			name: tool.definition.name,
			description: tool.definition.description,
			parameters: zodToJsonSchema(tool.definition.parameters),
			examples: tool.definition.examples
		}));
	}

	// Generate Anthropic tool schema for native tool use
	toAnthropicToolSchema(): Array<{
		name: string;
		description: string;
		input_schema: {
			type: 'object';
			properties: Record<string, any>;
			required?: string[];
		};
	}> {
		return this.getAll().map((tool) => {
			const jsonSchema = zodToJsonSchema(tool.definition.parameters) as any;

			// Convert Zod JSON Schema to Anthropic format
			return {
				name: tool.definition.name,
				description: tool.definition.description,
				input_schema: {
					type: 'object',
					properties: jsonSchema.properties || {},
					required: jsonSchema.required || []
				}
			};
		});
	}
}

export const toolRegistry = new ToolRegistry();
