import { z } from 'zod';
import type { Tool } from '$lib/types/tools';

const respondSchema = z.object({
	data: z.record(z.string(), z.any()).describe('Structured data for user-facing LLM to format'),
	summary: z.string().optional().describe('Optional summary of findings'),
	confidence: z.enum(['high', 'medium', 'low']).optional(),
	sources: z.array(z.string()).optional().describe('Data sources used')
});

export const respondTool: Tool = {
	definition: {
		name: 'respond',
		description:
			'Return final structured data to be formatted for the user. Use this when you have gathered all necessary information and are ready to provide the answer.',
		parameters: respondSchema,
		security: {
			rateLimit: { maxCallsPerMinute: 60, maxCallsPerHour: 1000 },
			auditLog: false,
			timeout: 1000
		},
		examples: [
			{
				scenario: "User asks \"What's the weather in NYC?\"",
				call: {
					tool: 'respond',
					parameters: {
						data: {
							location: 'New York City',
							temperature: 72,
							condition: 'sunny',
							humidity: 45
						},
						summary: 'Current weather in NYC is sunny and 72°F',
						confidence: 'high',
						sources: ['weather.com API']
					}
				},
				expectedResult: 'User-facing LLM formats this naturally'
			}
		]
	},

	async execute(parameters, context) {
		// Respond tool just returns the data as-is
		return {
			success: true,
			data: parameters,
			metadata: {
				duration: 0,
				cached: false,
				timestamp: Date.now()
			}
		};
	}
};
