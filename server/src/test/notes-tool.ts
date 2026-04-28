import assert from 'node:assert/strict'
import fs from 'fs/promises'
import os from 'os'
import path from 'path'
import { ToolRegistryImpl } from '../tools/registry.js'
import { registerNoteTools } from '../tools/notes.js'
import { materializeTextOutput } from '../orchestrator/output.js'

const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'notes-tool-test-'))
const notesDir = path.join(tmpDir, 'notes')
const sessionFilesRoot = path.join(tmpDir, 'sessions')

const registry = new ToolRegistryImpl()
registerNoteTools(registry, { notesDir, sessionFilesRoot })

const ctx = {
  agent_id: 'agent',
  session_id: 'session',
  signal: new AbortController().signal,
}

const saved = await registry.execute('notes.save_research_note', {
  title: 'OpenRouter Web Search: Phase 2?',
  markdown: '# Findings\n\n- Source-backed result\n\n## Sources\n- https://example.com/source',
}, ctx)

assert.equal(saved.ok, true)
const output = saved.output as { path: string }
assert.equal(output.path, '@note/openrouter-web-search-phase-2.md')
assert.equal(
  await fs.readFile(path.join(notesDir, 'openrouter-web-search-phase-2.md'), 'utf-8'),
  '# Findings\n\n- Source-backed result\n\n## Sources\n- https://example.com/source\n',
)

const rejectedNoUrl = await registry.execute('notes.save_research_note', {
  title: 'No URL',
  markdown: '# Findings\n\n- Source-backed result',
}, ctx)

assert.equal(rejectedNoUrl.ok, false)
assert.match(String(rejectedNoUrl.error), /raw http\(s\) source URL/)

const rejectedPlaceholderCitation = await registry.execute('notes.save_research_note', {
  title: 'Placeholder Citation',
  markdown: '# Findings\n\n- Source-backed result turn0search0\n\n## Sources\n- https://example.com/source',
}, ctx)

assert.equal(rejectedPlaceholderCitation.ok, false)
assert.match(String(rejectedPlaceholderCitation.error), /placeholder citations/)

const rejectedArtifact = await registry.execute('notes.save_research_note', {
  title: 'Artifact Leak',
  markdown: '# Findings\n\n- Source-backed result from artifact://agent/call-web.fetch.json\n\n## Sources\n- https://example.com/source',
}, ctx)

assert.equal(rejectedArtifact.ok, false)
assert.match(String(rejectedArtifact.error), /artifact references/)

const rejected = await registry.execute('notes.save_research_note', {
  title: 'Escape',
  markdown: '# Nope\n\nhttps://example.com/source',
  path: path.join(tmpDir, '..', 'escape.md'),
}, ctx)

assert.equal(rejected.ok, false)
assert.match(String(rejected.error), /path is not supported/)

const rejectedAbsoluteFilename = await registry.execute('notes.save_research_note', {
  title: 'Escape',
  markdown: '# Nope\n\nhttps://example.com/source',
  filename: path.join(tmpDir, 'escape.md'),
}, ctx)

assert.equal(rejectedAbsoluteFilename.ok, false)
assert.match(String(rejectedAbsoluteFilename.error), /relative file name/)

const artifactNotice = await materializeTextOutput(
  '# Promoted Findings\n\nA persisted report.\n\n## Sources\n- https://example.com/promoted\n',
  {
    sessionFilesRoot,
    inlineLimitBytes: 1,
    sessionId: ctx.session_id,
    agentId: ctx.agent_id,
    callId: 'call-1',
    toolName: 'delegate-researcher',
    extension: 'md',
  },
)
const artifactRef = artifactNotice.match(/artifact:\/\/\S+/)?.[0]
assert.equal(artifactRef, 'artifact://agent/call-1-delegate-researcher.md')

const promoted = await registry.execute('notes.promote', {
  from: artifactRef,
  title: 'Promoted Report',
  profile: 'research',
}, ctx)

assert.equal(promoted.ok, true)
const promotedOutput = promoted.output as { path: string; source_path: string; profile: string }
assert.equal(promotedOutput.path, '@note/promoted-report.md')
assert.equal(promotedOutput.source_path, artifactRef)
assert.equal(promotedOutput.profile, 'research')
assert.equal(
  await fs.readFile(path.join(notesDir, 'promoted-report.md'), 'utf-8'),
  '# Promoted Findings\n\nA persisted report.\n\n## Sources\n- https://example.com/promoted\n',
)

const genericArtifactNotice = await materializeTextOutput('Plain operational note without a URL', {
  sessionFilesRoot,
  inlineLimitBytes: 1,
  sessionId: ctx.session_id,
  agentId: ctx.agent_id,
  callId: 'call-2',
  toolName: 'delegate-default',
  extension: 'md',
})
const genericArtifactRef = genericArtifactNotice.match(/artifact:\/\/\S+/)?.[0]

const promotedGeneric = await registry.execute('notes.promote', {
  from: genericArtifactRef,
  title: 'Generic Note',
}, ctx)

assert.equal(promotedGeneric.ok, true)
assert.equal((promotedGeneric.output as { path: string }).path, '@note/generic-note.md')

const rejectedResearchPromote = await registry.execute('notes.promote', {
  from: genericArtifactRef,
  title: 'Research Without URL',
  profile: 'research',
}, ctx)

assert.equal(rejectedResearchPromote.ok, false)
assert.match(String(rejectedResearchPromote.error), /raw http\(s\) source URL/)

console.log('Notes tool tests passed')
