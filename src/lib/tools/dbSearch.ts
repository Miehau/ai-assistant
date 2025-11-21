import { z } from 'zod';
import type { Tool } from '$lib/types/tools';

const dbSearchSchema = z.object({
	query: z.string().describe('Search query - can be a user name (e.g., "John Doe"), email (e.g., "user@example.com"), or user ID (e.g., "123")'),
	limit: z.number().optional().describe('Maximum number of results (default: 10)')
});

export const dbSearchTool: Tool = {
	definition: {
		name: 'db_search',
		description: `Search the database for user information.

**Database Schema:**
- **Users** table
  - id (number): Unique user identifier
  - name (string): User's full name
  - email (string): User's email address

**Query Guidelines:**
- Search by name: "John" or "John Doe"
- Search by email: "user@example.com" or "@gmail.com"
- Search by ID: "123"

Use this when the user asks to search, find, or lookup user data.`,
		parameters: dbSearchSchema,
		security: {
			rateLimit: { maxCallsPerMinute: 30, maxCallsPerHour: 500 },
			auditLog: true,
			timeout: 3000
		},
		examples: [
			{
				scenario: 'User asks "find user John Doe"',
				call: {
					tool: 'db_search',
					parameters: {
						query: 'John Doe',
						limit: 10
					}
				},
				expectedResult: 'Users matching name "John Doe"'
			},
			{
				scenario: 'User asks "lookup email user@example.com"',
				call: {
					tool: 'db_search',
					parameters: {
						query: 'user@example.com'
					}
				},
				expectedResult: 'User with email user@example.com'
			},
			{
				scenario: 'User asks "get user with id 123"',
				call: {
					tool: 'db_search',
					parameters: {
						query: '123',
						limit: 1
					}
				},
				expectedResult: 'User with ID 123'
			}
		]
	},

	async execute(parameters, context) {
		// Simulate a small delay like a real database query
		await new Promise((resolve) => setTimeout(resolve, 300));

		// Return a hardcoded haiku about clouds
		const haiku = `Soft clouds drift slowly
Painting the azure canvas
Dreams float through the sky`;

		return {
			success: true,
			data: {
				query: parameters.query,
				limit: parameters.limit || 10,
				results: [
					{
						id: 1,
						type: 'haiku',
						content: haiku,
						author: 'Database Poet',
						tags: ['clouds', 'sky', 'nature'],
						created_at: new Date().toISOString()
					}
				],
				total_found: 1,
				search_time_ms: 300
			},
			metadata: {
				duration: 300,
				cached: false,
				timestamp: Date.now()
			}
		};
	}
};
