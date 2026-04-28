import assert from 'node:assert/strict'
import fs from 'fs/promises'
import os from 'os'
import path from 'path'
import { ToolRegistryImpl } from '../tools/registry.js'
import { registerFileTools } from '../tools/files.js'
import { registerSearchTools } from '../tools/search.js'
import { registerNoteTools } from '../tools/notes.js'
import { materializeTextOutput } from '../orchestrator/output.js'
import { deleteSessionFiles, resolveManagedFilePath } from '../tools/path-policy.js'

const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'session-files-test-'))
const sessionFilesRoot = path.join(tmpDir, 'sessions')
const notesDir = path.join(tmpDir, 'notes')
const sessionId = 'session-123'
const agentId = 'agent-123'
const ctx = {
  agent_id: agentId,
  session_id: sessionId,
  signal: new AbortController().signal,
}

const resolverOptions = {
  sessionFilesRoot,
  notesDir,
  sessionId,
  access: 'read' as const,
}

assert.equal(
  resolveManagedFilePath('drafts/work.md', resolverOptions).fsPath,
  path.join(sessionFilesRoot, sessionId, 'workspace', 'drafts', 'work.md'),
)
assert.equal(
  resolveManagedFilePath('artifact://agent/output.md', resolverOptions).fsPath,
  path.join(sessionFilesRoot, sessionId, 'artifacts', 'agent', 'output.md'),
)
assert.equal(
  resolveManagedFilePath('@note/research.md', resolverOptions).fsPath,
  path.join(notesDir, 'research.md'),
)
assert.equal(
  resolveManagedFilePath('@note', resolverOptions).fsPath,
  notesDir,
)
assert.throws(() => resolveManagedFilePath('/tmp/escape.md', resolverOptions), /Absolute paths/)
assert.throws(() => resolveManagedFilePath('drafts/../escape.md', resolverOptions), /traversal/)
assert.throws(() => resolveManagedFilePath('https://example.com/file.md', resolverOptions), /Unsupported path scheme/)

const registry = new ToolRegistryImpl()
registerFileTools(registry, { sessionFilesRoot, notesDir })
registerSearchTools(registry, { sessionFilesRoot, notesDir })
registerNoteTools(registry, { notesDir, sessionFilesRoot })

const writeResult = await registry.execute('files.write', {
  path: 'drafts/work.md',
  content: 'first line\nworkspace needle\n',
}, ctx)
assert.equal(writeResult.ok, true)
assert.equal((writeResult.output as { path: string }).path, 'drafts/work.md')

const workspaceSearch = await registry.execute('search', {
  query: 'workspace needle',
  path: 'drafts',
  literal: true,
}, ctx)
assert.equal(workspaceSearch.ok, true)
assert.equal((workspaceSearch.output as { count: number }).count, 1)

const artifactNotice = await materializeTextOutput('alpha\nartifact needle\nomega', {
  sessionFilesRoot,
  inlineLimitBytes: 1,
  sessionId,
  agentId,
  callId: 'call-1',
  toolName: 'research',
  extension: 'md',
})
const artifactRef = artifactNotice.match(/artifact:\/\/\S+/)?.[0]
assert.equal(artifactRef, 'artifact://agent-123/call-1-research.md')

const artifactRead = await registry.execute('files.read', {
  path: artifactRef,
  start_line: 2,
  end_line: 2,
}, ctx)
assert.equal(artifactRead.ok, true)
assert.match((artifactRead.output as { content: string }).content, /2\tartifact needle/)

const artifactSearch = await registry.execute('search', {
  query: 'artifact needle',
  path: artifactRef,
  literal: true,
}, ctx)
assert.equal(artifactSearch.ok, true)
const artifactSearchOutput = artifactSearch.output as { count: number; matches: Array<{ path: string }> }
assert.equal(artifactSearchOutput.count, 1)
assert.equal(artifactSearchOutput.matches[0].path, artifactRef)

const noteSave = await registry.execute('notes.save_research_note', {
  title: 'Logical Ref Note',
  markdown: '# Note\n\nnote needle\n\n## Sources\n- https://example.com/source\n',
}, ctx)
assert.equal(noteSave.ok, true)
const noteRef = (noteSave.output as { path: string }).path
assert.equal(noteRef, '@note/logical-ref-note.md')

const noteRead = await registry.execute('files.read', { path: noteRef }, ctx)
assert.equal(noteRead.ok, true)
assert.match((noteRead.output as { content: string }).content, /note needle/)

const noteSearch = await registry.execute('search', {
  query: 'note needle',
  path: noteRef,
  literal: true,
}, ctx)
assert.equal(noteSearch.ok, true)
assert.equal((noteSearch.output as { count: number }).count, 1)

const artifactWrite = await registry.execute('files.write', {
  path: artifactRef,
  content: 'nope',
}, ctx)
assert.equal(artifactWrite.ok, false)
assert.match(String(artifactWrite.error), /read-only/)

const noteWrite = await registry.execute('files.write', {
  path: noteRef,
  content: 'nope',
}, ctx)
assert.equal(noteWrite.ok, false)
assert.match(String(noteWrite.error), /read-only/)

await deleteSessionFiles(sessionFilesRoot, sessionId)
await assert.rejects(
  () => fs.stat(path.join(sessionFilesRoot, sessionId)),
  /ENOENT/,
)

console.log('Session file reference tests passed')
