import { createCipheriv, createDecipheriv, randomBytes, createHash } from 'node:crypto'

const ALGO = 'aes-256-gcm'
const IV_LEN = 12
const TAG_LEN = 16
const ENCRYPTED_PREFIX = 'enc::'

/**
 * Derive a 32-byte hex key from an arbitrary-length secret.
 */
export function deriveKey(secret: string): string {
  return createHash('sha256').update(secret).digest('hex')
}

/* eslint-disable @typescript-eslint/no-explicit-any --
   Node 22 @types/node has a known Buffer/Uint8Array incompatibility with crypto APIs.
   Buffer works at runtime; the type system disagrees. */

export function encrypt(plaintext: string, hexKey: string): string {
  const key = Buffer.from(hexKey, 'hex')
  const iv = randomBytes(IV_LEN)
  const cipher = (createCipheriv as any)(ALGO, key, iv)
  const enc = Buffer.concat([cipher.update(plaintext, 'utf8'), cipher.final()])
  const tag = cipher.getAuthTag()
  const packed = Buffer.concat([iv, tag, enc]).toString('base64')
  return ENCRYPTED_PREFIX + packed
}

export function decrypt(stored: string, hexKey: string): string {
  if (!stored.startsWith(ENCRYPTED_PREFIX)) {
    return stored
  }

  const key = Buffer.from(hexKey, 'hex')
  const packed = stored.slice(ENCRYPTED_PREFIX.length)
  const buf = Buffer.from(packed, 'base64')
  const iv = buf.subarray(0, IV_LEN)
  const tag = buf.subarray(IV_LEN, IV_LEN + TAG_LEN)
  const encrypted = buf.subarray(IV_LEN + TAG_LEN)
  const decipher = (createDecipheriv as any)(ALGO, key, iv)
  decipher.setAuthTag(tag)
  return Buffer.concat([decipher.update(encrypted), decipher.final()]).toString('utf8')
}

/* eslint-enable @typescript-eslint/no-explicit-any */

export function isEncrypted(stored: string): boolean {
  return stored.startsWith(ENCRYPTED_PREFIX)
}
