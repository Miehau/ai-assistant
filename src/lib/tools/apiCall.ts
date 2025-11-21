import { z } from 'zod';
import type { Tool } from '$lib/types/tools';

const apiCallSchema = z.object({
	url: z.string().url().describe('API endpoint URL'),
	method: z.enum(['GET', 'POST', 'PUT', 'DELETE']).describe('HTTP method'),
	headers: z.record(z.string(), z.string()).optional().describe('Request headers'),
	body: z.any().optional().describe('Request body for POST/PUT'),
	timeout: z.number().optional().describe('Request timeout in ms (default: 10000)')
});

const DEFAULT_ALLOWED_DOMAINS = [
	'api.openweathermap.org',
	'api.github.com',
	'jsonplaceholder.typicode.com'
	// Add more allowed domains
];

export const apiCallTool: Tool = {
	definition: {
		name: 'api_call',
		description:
			'Make an HTTP API call to fetch data from external services. Only works with whitelisted domains for security.',
		parameters: apiCallSchema,
		security: {
			urlAllowlist: DEFAULT_ALLOWED_DOMAINS,
			urlBlocklist: [
				'localhost',
				'127.0.0.1',
				'0.0.0.0'
				// Block internal IPs (SSRF prevention)
			],
			rateLimit: {
				maxCallsPerMinute: 10,
				maxCallsPerHour: 100
			},
			auditLog: true,
			maxResponseSize: 1024 * 1024, // 1MB
			timeout: 10000
		},
		examples: [
			{
				scenario: 'Fetch weather data',
				call: {
					tool: 'api_call',
					parameters: {
						url: 'https://api.openweathermap.org/data/2.5/weather?q=London&appid=KEY',
						method: 'GET'
					}
				},
				expectedResult: 'JSON weather data'
			}
		]
	},

	async execute(parameters, context) {
		const { url, method, headers, body, timeout } = parameters;

		// Security: Validate URL against allowlist
		const urlObj = new URL(url);
		const allowed = this.definition.security.urlAllowlist || [];
		const blocked = this.definition.security.urlBlocklist || [];

		if (blocked.some((domain) => urlObj.hostname.includes(domain))) {
			throw new Error(`Blocked domain: ${urlObj.hostname}`);
		}

		if (allowed.length > 0 && !allowed.some((domain) => urlObj.hostname.includes(domain))) {
			throw new Error(`Domain not in allowlist: ${urlObj.hostname}`);
		}

		// Make request
		const controller = new AbortController();
		const timeoutId = setTimeout(
			() => controller.abort(),
			timeout || this.definition.security.timeout
		);

		try {
			const response = await fetch(url, {
				method,
				headers: {
					'Content-Type': 'application/json',
					...headers
				},
				body: body ? JSON.stringify(body) : undefined,
				signal: controller.signal
			});

			clearTimeout(timeoutId);

			// Check response size
			const contentLength = response.headers.get('content-length');
			const maxSize = this.definition.security.maxResponseSize!;
			if (contentLength && parseInt(contentLength) > maxSize) {
				throw new Error(`Response too large: ${contentLength} bytes (max: ${maxSize})`);
			}

			if (!response.ok) {
				throw new Error(`HTTP ${response.status}: ${response.statusText}`);
			}

			const contentType = response.headers.get('content-type');
			let data;

			if (contentType?.includes('application/json')) {
				data = await response.json();
			} else {
				data = await response.text();
			}

			return {
				success: true,
				data: {
					status: response.status,
					headers: Object.fromEntries(response.headers.entries()),
					body: data
				},
				metadata: {
					duration: 0, // Set in executor
					cached: false,
					timestamp: Date.now()
				}
			};
		} catch (error: any) {
			if (error.name === 'AbortError') {
				throw new Error('Request timeout');
			}
			throw error;
		}
	}
};
