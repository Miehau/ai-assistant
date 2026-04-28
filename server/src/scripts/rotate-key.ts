import 'dotenv/config'
import { randomBytes, createHash } from 'node:crypto'
import { loadConfig } from '../lib/config.js'
import { openDatabase } from '../repositories/factory.js'

async function main() {
  const config = loadConfig()
  const opened = await openDatabase(config)

  try {
    const users = await opened.repositories.users.list()
    if (users.length === 0) {
      console.error('No user exists. Use `bun run create-key` to create one.')
      process.exit(1)
    }
    if (users.length > 1) {
      console.error(`Expected exactly one user, found ${users.length}. Refusing to rotate ambiguously.`)
      process.exit(1)
    }

    const token = randomBytes(32).toString('hex')
    const hash = createHash('sha256').update(token).digest('hex')
    const updated = await opened.repositories.users.setApiKeyHash(users[0].id, hash)

    console.log()
    console.log('Token rotated. The previous token is now invalid.')
    console.log()
    console.log(`  User ID: ${updated.id}`)
    console.log(`  Token:   ${token}`)
    console.log()
  } finally {
    await opened.close()
  }
}

main().catch((err) => {
  console.error('Failed:', err instanceof Error ? err.message : err)
  process.exit(1)
})
