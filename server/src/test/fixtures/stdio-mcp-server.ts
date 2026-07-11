import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'

const server = new McpServer({ name: 'stdio-fixture', version: '1.0.0' })
server.registerTool('ping', { inputSchema: { value: z.string().optional() } }, async ({ value }) => ({
  content: [{ type: 'text', text: value ? `pong:${value}` : 'pong' }],
}))
await server.connect(new StdioServerTransport())
