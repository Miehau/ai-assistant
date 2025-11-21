import type { Tool, ToolResult, ToolSchema } from "./types";
import type { JSONSchema } from "$lib/types/llm";

export class ProcessHtmlTool implements Tool {
    name: string = "fetch_from_web";
    description: string = "Fetches the content of a web page";
    input_schema: JSONSchema = {
        type: 'object',
        properties: {
            htmlContent: {
                type: 'string',
                description: "The HTML content of the web page to fetch"
            }
        },
        required: ['htmlContent'],
        additionalProperties: false
    };
    toSchema(): ToolSchema {
        return {
            name: this.name,
            description: this.description,
            input_schema: this.input_schema,
            strict: true
        };
    }
    async execute(input: any): Promise<ToolResult> {
        return {
            success: true,
            result: input.htmlContent
        };
    }
}