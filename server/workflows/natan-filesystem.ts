import { z } from 'zod'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { WorkflowDefinition, WorkflowContext } from '../src/workflows/types.js'

const inputSchema = z.object({}).default({})

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const API_URL = process.env.VERIFY_URL ?? ''
const API_KEY = process.env.VERIFY_API_KEY ?? ''
const TASK = 'filesystem'
const NOTES_DIR = '***REMOVED***Projects/4th-devs/tasks/s0301'

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

/** Strip Polish diacritics to ASCII equivalents */
const PL_MAP: Record<string, string> = {
  ą: 'a', ć: 'c', ę: 'e', ł: 'l', ń: 'n', ó: 'o', ś: 's', ź: 'z', ż: 'z',
  Ą: 'A', Ć: 'C', Ę: 'E', Ł: 'L', Ń: 'N', Ó: 'O', Ś: 'S', Ź: 'Z', Ż: 'Z',
}
const PL_RE = new RegExp(`[${Object.keys(PL_MAP).join('')}]`, 'g')
function stripPl(s: string): string {
  return s.replace(PL_RE, (ch) => PL_MAP[ch] ?? ch)
}

/** transakcje.txt: "SellerCity -> good -> BuyerCity" per line */
function parseTransakcje(text: string): { seller: string; good: string; buyer: string }[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.includes(' -> '))
    .map((line) => {
      const [seller, good, buyer] = line.split(' -> ')
      return { seller: seller.trim(), good: good.trim(), buyer: buyer.trim() }
    })
}

/** Derive towary: good → unique seller cities */
function deriveTowary(transactions: { seller: string; good: string }[]): Record<string, string[]> {
  const towary: Record<string, string[]> = {}
  for (const { seller, good } of transactions) {
    if (!towary[good]) towary[good] = []
    if (!towary[good].includes(seller)) towary[good].push(seller)
  }
  return towary
}

const EXTRACTION_PROMPT = `You receive two Polish-language text files about trade between cities.

Extract:
1. From "ogłoszenia.txt": what each city needs — goods with quantities (numbers only, no units). Use nominative singular for all goods and city names.
2. From "rozmowy.txt": which person is responsible for trade in which city. Each city has exactly ONE responsible person. Cross-reference ALL entries mentioning the same city to collect name fragments — a first name may appear in one entry and a surname in a different entry. Combine them into a single "FirstName Surname". Every person MUST have both first name and surname. Use nominative for city names.

IMPORTANT: Do NOT use Polish diacritics (ą, ć, ę, ł, ń, ó, ś, ź, ż) anywhere in the output.
Replace them with ASCII equivalents (a, c, e, l, n, o, s, z, z).

Respond with ONLY a JSON object:
{
  "miasta": { "CityName": { "good": quantity, ... }, ... },
  "osoby": [{ "name": "Full Name", "city": "CityName" }, ...]
}`

// ---------------------------------------------------------------------------
// API helper
// ---------------------------------------------------------------------------

type ApiAction =
  | { action: 'help' }
  | { action: 'reset' }
  | { action: 'done' }
  | { action: 'createDirectory'; path: string }
  | { action: 'createFile'; path: string; content: string }

async function callApi(ctx: WorkflowContext, answer: ApiAction | ApiAction[]): Promise<unknown> {
  return ctx.tool('web.request', {
    method: 'POST',
    url: API_URL,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ apikey: API_KEY, task: TASK, answer }),
  })
}

// ---------------------------------------------------------------------------
// Workflow
// ---------------------------------------------------------------------------

export const workflow: WorkflowDefinition = {
  name: 'natan-filesystem',
  description: "Build the filesystem structure from Natan's trade notes on the ag3nts hub API.",
  inputSchema,
  tools: ['web.request'],

  async run(ctx) {
    // Step 1: Read note files from disk
    const [ogloszenia, rozmowy, transakcje] = await ctx.step('reading', () =>
      Promise.all([
        readFile(join(NOTES_DIR, 'ogłoszenia.txt'), 'utf-8'),
        readFile(join(NOTES_DIR, 'rozmowy.txt'), 'utf-8'),
        readFile(join(NOTES_DIR, 'transakcje.txt'), 'utf-8'),
      ])
    )

    // Step 2: Parse transakcje deterministically
    const transactions = parseTransakcje(transakcje)
    const towary = deriveTowary(transactions)

    // Step 3: Extract miasta + osoby via single LLM call
    const extracted = await ctx.step('extracting', async () => {
      const llmResult = await ctx.llm({
        prompt: `${EXTRACTION_PROMPT}\n\n--- ogłoszenia.txt ---\n${ogloszenia}\n\n--- rozmowy.txt ---\n${rozmowy}`,
        model: 'openrouter:openai/gpt-5.4-nano',
      })

      const text = typeof llmResult === 'string' ? llmResult : JSON.stringify(llmResult)
      const jsonStr = text.replace(/```json?\n?/g, '').replace(/```\n?/g, '').trim()
      return JSON.parse(jsonStr) as {
        miasta: Record<string, Record<string, number>>
        osoby: { name: string; city: string }[]
      }
    })

    // Step 4: Call help to inspect API requirements
    const helpResult = await ctx.step('help', () => callApi(ctx, { action: 'help' }))

    // Step 5: Reset remote filesystem
    await ctx.step('reset', () => callApi(ctx, { action: 'reset' }))

    // Step 5: Create directories first (must exist before files)
    await ctx.step('creating_dirs', () =>
      callApi(ctx, [
        { action: 'createDirectory', path: '/miasta' },
        { action: 'createDirectory', path: '/osoby' },
        { action: 'createDirectory', path: '/towary' },
      ])
    )

    // Step 6: Build file operations
    // API requires: ^[a-z0-9_]+$ for names, no extensions, max 20 chars
    const safePath = (s: string) => stripPl(s).toLowerCase().replace(/ /g, '_')

    const fileActions: ApiAction[] = []

    for (const [city, needs] of Object.entries(extracted.miasta)) {
      const safeNeeds: Record<string, number> = {}
      for (const [good, qty] of Object.entries(needs)) safeNeeds[safePath(good)] = qty
      fileActions.push({
        action: 'createFile',
        path: `/miasta/${safePath(city)}`,
        content: JSON.stringify(safeNeeds),
      })
    }

    for (const { name, city } of extracted.osoby) {
      fileActions.push({
        action: 'createFile',
        path: `/osoby/${safePath(name)}`,
        content: `${stripPl(name)} [${stripPl(city)}](/miasta/${safePath(city)})`,
      })
    }

    for (const [good, sellers] of Object.entries(towary)) {
      const links = sellers.map((city) => {
        return `[${stripPl(city)}](/miasta/${safePath(city)})`
      }).join('\n')
      fileActions.push({
        action: 'createFile',
        path: `/towary/${safePath(good)}`,
        content: links,
      })
    }

    // Step 7: Send files batch
    const uploadResult = await ctx.step('uploading', () => callApi(ctx, fileActions))

    // Step 8: Submit for verification
    const doneResult = await ctx.step('submitting', () => callApi(ctx, { action: 'done' }))

    return {
      extracted,
      towary,
      uploadResult,
      doneResult,
      stats: {
        cities: Object.keys(extracted.miasta).length,
        people: extracted.osoby.length,
        goods: Object.keys(towary).length,
        files: fileActions.length,
      },
    }
  },
}
