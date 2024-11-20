import { Langfuse } from 'langfuse';
import type { ChatCompletionMessageParam, ChatCompletion } from "openai/resources/chat/completions";

export class LangfuseService {
  public langfuse: Langfuse;

  constructor() {
    console.log(import.meta.env);
    this.langfuse = new Langfuse({
      secretKey: import.meta.env.VITE_LANGFUSE_API_KEY,
      publicKey: import.meta.env.VITE_LANGFUSE_PUBLIC_KEY,
      baseUrl: import.meta.env.VITE_LANGFUSE_HOST
    });

    this.langfuse.on("error", (error: Error) => {
      console.error("Langfuse error:", error);
    });

    if (process.env.NODE_ENV === 'development') {
      this.langfuse.debug();
    }
  }

  flushAsync(): Promise<void> {
    return this.langfuse.flushAsync();
  }

  createTrace(options: { id: string, name: string, sessionId: string }): any {
    return this.langfuse.trace(options);
  }

  createSpan(trace: any, name: string, input?: any): any {
    return trace.span({ name, input: input ? JSON.stringify(input) : undefined });
  }

  finalizeSpan(span: any, name: string, input: any, output: any): void {
    span.update({
      name,
      output: JSON.stringify(output),
    });
    span.end();
  }

  async finalizeTrace(trace: any, input: any, output: any): Promise<void> {
    await trace.update({ 
      input: JSON.stringify(input),
      output: JSON.stringify(output),
    });
    await this.langfuse.flushAsync();
  }

  async shutdownAsync(): Promise<void> {
    await this.langfuse.shutdownAsync();
  }

  createGeneration(trace: any, name: string, input: any): any {
    return trace.generation({
      name,
      input: JSON.stringify(input),
    });
  }

  createEvent(trace: any, name: string, input?: any, output?: any): void {
    trace.event({
      name,
      input: input ? JSON.stringify(input) : undefined,
      output: output ? JSON.stringify(output) : undefined,
    });
  }

  finalizeGeneration(generation: any, output: any, model: string, usage?: { promptTokens?: number, completionTokens?: number, totalTokens?: number }): void {
    generation.update({
      output: JSON.stringify(output),
      model,
      usage,
    });
    generation.end();
  }

  // Remove the finalizeEvent method as it's not needed for LangfuseEventClient
}