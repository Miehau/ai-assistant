import type { Tool, ToolResult } from "./types";

export class ProcessHtmlTool implements Tool {
    name: string = "fetch_from_web";
    description: string = "Fetches the content of a web page";
    parameters: Record<string, any> = {
        htmlContent: {
            type: "string",
            description: "The HTML content of the web page to fetch"
        }
    };
    toSchema: () => string = () => JSON.stringify({
        "name": this.name,
        "description": this.description,
        "parameters": this.parameters
    });
    execute: (input: any) => Promise<ToolResult> = async (input: any) => {
        return {
            success: true,
            result: input.htmlContent
        };
    }
}