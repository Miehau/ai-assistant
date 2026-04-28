import 'dotenv/config'
import { randomBytes, createHash } from 'node:crypto'
import { loadConfig } from '../lib/config.js'
import { openDatabase } from '../repositories/factory.js'

async function main() {
  const config = loadConfig()
  const opened = await openDatabase(config)

  try {
    const existing = await opened.repositories.users.list()
    if (existing.length > 0) {
      console.error(`A user already exists (${existing[0].id}). Use \`bun run rotate-key\` to rotate the token.`)
      process.exit(1)
    }

    const token = randomBytes(32).toString('hex')
    const hash = createHash('sha256').update(token).digest('hex')
    const user = await opened.repositories.users.create({ email: null, apiKeyHash: hash })

    console.log()
    console.log('User created. Store the token now — it cannot be recovered.')
    console.log()
    console.log(`  User ID: ${user.id}`)
    console.log(`  Token:   ${token}`)
    console.log()
    console.log('Use this token as `Authorization: Bearer <token>` from your client.')
  } finally {
    await opened.close()
  }
}

main().catch((err) => {
  console.error('Failed:', err instanceof Error ? err.message : err)
  process.exit(1)
})
