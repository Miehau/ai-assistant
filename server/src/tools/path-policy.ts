import fs from 'fs/promises'
import path from 'path'

export type ManagedFileKind = 'workspace' | 'artifact' | 'note'
export type ManagedFileAccess = 'read' | 'write'

export interface ManagedFileOptions {
  sessionFilesRoot: string
  notesDir: string
  sessionId: string
  access: ManagedFileAccess
}

export interface ResolvedManagedFile {
  kind: ManagedFileKind
  fsPath: string
  rootPath: string
  logicalPath: string
  ref: string
}

const ARTIFACT_PREFIX = 'artifact://'
const NOTE_PREFIX = 'note://'
const UNKNOWN_SCHEME_RE = /^[a-z][a-z0-9+.-]*:\/\//i
const WINDOWS_ABSOLUTE_RE = /^[a-zA-Z]:[\\/]/

export function resolveManagedFilePath(
  requestedPath: string,
  options: ManagedFileOptions,
): ResolvedManagedFile {
  if (typeof requestedPath !== 'string' || requestedPath.trim() === '') {
    throw new Error('path is required')
  }

  const rawPath = requestedPath.trim()
  if (path.isAbsolute(rawPath) || WINDOWS_ABSOLUTE_RE.test(rawPath)) {
    throw new Error('Absolute paths are not accepted; use a relative path, artifact://, or note://')
  }

  const sessionRoot = getSessionFilesDir(options.sessionFilesRoot, options.sessionId)
  const artifactRoot = path.join(sessionRoot, 'artifacts')
  const workspaceRoot = path.join(sessionRoot, 'workspace')
  const notesRoot = path.resolve(options.notesDir)

  let kind: ManagedFileKind
  let rootPath: string
  let logicalPath: string

  if (rawPath.startsWith(ARTIFACT_PREFIX)) {
    kind = 'artifact'
    rootPath = artifactRoot
    logicalPath = normalizeLogicalPath(rawPath.slice(ARTIFACT_PREFIX.length))
  } else if (rawPath.startsWith(NOTE_PREFIX)) {
    kind = 'note'
    rootPath = notesRoot
    logicalPath = normalizeLogicalPath(rawPath.slice(NOTE_PREFIX.length))
  } else {
    if (UNKNOWN_SCHEME_RE.test(rawPath)) {
      throw new Error('Unsupported path scheme; use artifact://, note://, or a relative path')
    }
    kind = 'workspace'
    rootPath = workspaceRoot
    logicalPath = normalizeLogicalPath(rawPath)
  }

  if (options.access === 'write' && kind !== 'workspace') {
    throw new Error(`${kind} paths are read-only; write to a relative session workspace path`)
  }

  const fsPath = resolveInsideRoot(rootPath, logicalPath)
  return {
    kind,
    fsPath,
    rootPath: path.resolve(rootPath),
    logicalPath,
    ref: buildManagedFileRef(kind, logicalPath),
  }
}

export function managedFileRefForPath(
  fsPath: string,
  resolved: Pick<ResolvedManagedFile, 'kind' | 'rootPath'>,
): string {
  const root = path.resolve(resolved.rootPath)
  const candidate = path.resolve(fsPath)
  if (!isUnderRoot(candidate, root)) {
    throw new Error(`Path is outside managed ${resolved.kind} root`)
  }

  const relative = path.relative(root, candidate).split(path.sep).join('/')
  return buildManagedFileRef(resolved.kind, relative)
}

export function joinManagedFileRef(baseRef: string, relativePath: string): string {
  const suffix = normalizeLogicalPath(relativePath)
  if (!suffix) return baseRef

  if (baseRef === '.') return suffix
  if (baseRef.endsWith('://')) return `${baseRef}${suffix}`
  return `${baseRef.replace(/\/+$/, '')}/${suffix}`
}

export function getSessionFilesDir(sessionFilesRoot: string, sessionId: string): string {
  return path.join(path.resolve(sessionFilesRoot), safePathPart(sessionId))
}

export async function deleteSessionFiles(sessionFilesRoot: string, sessionId: string): Promise<void> {
  await fs.rm(getSessionFilesDir(sessionFilesRoot, sessionId), {
    recursive: true,
    force: true,
  })
}

function normalizeLogicalPath(value: string): string {
  let decoded: string
  try {
    decoded = decodeURIComponent(value)
  } catch {
    throw new Error('Path contains invalid percent encoding')
  }

  if (decoded.includes('\0')) {
    throw new Error('Path contains a NUL byte')
  }
  if (decoded.includes('\\')) {
    throw new Error('Backslashes are not accepted in managed paths')
  }
  if (decoded.startsWith('/')) {
    throw new Error('Managed paths must be relative within their root')
  }

  const segments: string[] = []
  for (const segment of decoded.split('/')) {
    if (segment === '' || segment === '.') continue
    if (segment === '..') {
      throw new Error('Path traversal is not accepted')
    }
    segments.push(segment)
  }

  return segments.join('/')
}

function resolveInsideRoot(rootPath: string, logicalPath: string): string {
  const root = path.resolve(rootPath)
  const candidate = path.resolve(root, logicalPath)
  if (!isUnderRoot(candidate, root)) {
    throw new Error(`Path is outside managed root: ${root}`)
  }
  return candidate
}

function buildManagedFileRef(kind: ManagedFileKind, logicalPath: string): string {
  if (kind === 'artifact') return `${ARTIFACT_PREFIX}${logicalPath}`
  if (kind === 'note') return `${NOTE_PREFIX}${logicalPath}`
  return logicalPath || '.'
}

function isUnderRoot(candidate: string, root: string): boolean {
  return candidate === root || candidate.startsWith(`${root}${path.sep}`)
}

function safePathPart(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 96) || 'session'
}
