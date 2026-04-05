import { z } from 'zod'
import type { WorkflowDefinition, WorkflowContext } from '../src/workflows/types.js'

const inputSchema = z.object({
  /** Directory containing files to classify. */
  source_dir: z.string(),
  /** Base directory where classified files will be organized into subdirectories. */
  output_dir: z.string(),
  /** Classification prompt — tell the LLM how to categorize. */
  classification_prompt: z.string().optional(),
  /** Max files to process concurrently. */
  concurrency: z.number().min(1).max(10).default(3),
})

type Input = z.infer<typeof inputSchema>

interface ClassificationResult {
  file: string
  category: string
  summary: string
}

const DEFAULT_CLASSIFICATION_PROMPT = `You are a file classifier. Given the file name and a preview of its content, respond with a JSON object:
{ "category": "<short_snake_case_category>", "summary": "<one-sentence summary of the file>" }

Categories should be concise and descriptive (e.g. "config", "documentation", "source_code", "data", "test", "script", "image_asset", "unknown").
Only output the JSON, nothing else.`

export const workflow: WorkflowDefinition<Input> = {
  name: 'file-classifier',
  description: 'Classify files in a directory using LLM and organize them into categorized subdirectories.',
  inputSchema,
  tools: ['files.read', 'files.write', 'files.list', 'files.create'],

  async run(ctx: WorkflowContext<Input>) {
    const { source_dir, output_dir, classification_prompt, concurrency } = ctx.input

    // Step 1: List all files in source directory
    const listing = await ctx.step('listing', () =>
      ctx.tool('files.list', { path: source_dir, recursive: false }) as Promise<{
        entries: Array<{ name: string; type: string; size: number }>
      }>
    )

    const files = listing.entries.filter((e) => e.type === 'file')
    if (files.length === 0) {
      return { classified: 0, categories: {}, message: 'No files found in source directory.' }
    }

    // Step 2: Classify each file using LLM
    const prompt = classification_prompt ?? DEFAULT_CLASSIFICATION_PROMPT
    const results = await ctx.step('classifying', () =>
      ctx.map<typeof files[number], ClassificationResult>(
        files,
        async (file) => {
          let preview: string
          try {
            const read = (await ctx.tool('files.read', {
              path: `${source_dir}/${file.name}`,
              end_line: 50,
            })) as { content: string }
            preview = read.content
          } catch {
            preview = '(unable to read file content)'
          }

          const llmResult = await ctx.llm({
            prompt: `${prompt}\n\nFile: ${file.name} (${file.size} bytes)\n\nContent preview:\n${preview}`,
          })

          const text = typeof llmResult === 'string' ? llmResult : JSON.stringify(llmResult)
          try {
            const parsed = JSON.parse(text) as { category: string; summary: string }
            return { file: file.name, category: parsed.category, summary: parsed.summary }
          } catch {
            return { file: file.name, category: 'unknown', summary: 'Classification failed — could not parse LLM response' }
          }
        },
        { concurrency },
      )
    )

    // Step 3: Group by category and create output structure
    const categories: Record<string, ClassificationResult[]> = {}
    for (const result of results) {
      const cat = result.category || 'unknown'
      if (!categories[cat]) categories[cat] = []
      categories[cat].push(result)
    }

    // Step 4: Copy files into categorized directories
    await ctx.step('organizing', async () => {
      for (const [category, items] of Object.entries(categories)) {
        for (const item of items) {
          const sourcePath = `${source_dir}/${item.file}`
          const destPath = `${output_dir}/${category}/${item.file}`

          try {
            const content = (await ctx.tool('files.read', { path: sourcePath })) as { content: string }
            await ctx.tool('files.write', { path: destPath, content: content.content })
          } catch {
            // Best-effort — skip files that fail to copy
          }
        }
      }
    })

    // Step 5: Write a manifest
    const manifest = {
      source_dir,
      output_dir,
      classified_at: new Date().toISOString(),
      total_files: files.length,
      categories: Object.fromEntries(
        Object.entries(categories).map(([cat, items]) => [
          cat,
          items.map((i) => ({ file: i.file, summary: i.summary })),
        ]),
      ),
    }

    await ctx.step('writing_manifest', () =>
      ctx.tool('files.write', {
        path: `${output_dir}/manifest.json`,
        content: JSON.stringify(manifest, null, 2),
      })
    )

    return manifest
  },
}
