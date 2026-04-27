import assert from 'node:assert/strict'
import fs from 'fs/promises'
import os from 'os'
import path from 'path'
import { ToolRegistryImpl } from '../tools/registry.js'
import { registerNoteTools } from '../tools/notes.js'

const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'notes-tool-test-'))
const notesDir = path.join(tmpDir, 'notes')

const registry = new ToolRegistryImpl()
registerNoteTools(registry, notesDir)

const saved = await registry.execute('notes.save_research_note', {
  title: 'OpenRouter Web Search: Phase 2?',
  markdown: '# Findings\n\n- Source-backed result',
}, {
  agent_id: 'agent',
  session_id: 'session',
  signal: new AbortController().signal,
})

assert.equal(saved.ok, true)
const output = saved.output as { path: string }
assert.equal(output.path, 'note://openrouter-web-search-phase-2.md')
assert.equal(
  await fs.readFile(path.join(notesDir, 'openrouter-web-search-phase-2.md'), 'utf-8'),
  '# Findings\n\n- Source-backed result\n',
)

const rejected = await registry.execute('notes.save_research_note', {
  title: 'Escape',
  markdown: '# Nope',
  path: path.join(tmpDir, '..', 'escape.md'),
}, {
  agent_id: 'agent',
  session_id: 'session',
  signal: new AbortController().signal,
})

assert.equal(rejected.ok, false)
assert.match(String(rejected.error), /path is not supported/)

const rejectedAbsoluteFilename = await registry.execute('notes.save_research_note', {
  title: 'Escape',
  markdown: '# Nope',
  filename: path.join(tmpDir, 'escape.md'),
}, {
  agent_id: 'agent',
  session_id: 'session',
  signal: new AbortController().signal,
})

assert.equal(rejectedAbsoluteFilename.ok, false)
assert.match(String(rejectedAbsoluteFilename.error), /relative file name/)

console.log('Notes tool tests passed')
