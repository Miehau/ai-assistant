import { defineConfig } from 'drizzle-kit'

const dialect = (process.env.DB_DIALECT ?? 'sqlite') as 'sqlite' | 'postgres'
const databaseUrl = process.env.DATABASE_URL ?? './data/app.db'

export default dialect === 'postgres'
  ? defineConfig({
      schema: './src/db/schema-pg.ts',
      out: './drizzle/pg',
      dialect: 'postgresql',
      dbCredentials: { url: databaseUrl },
    })
  : defineConfig({
      schema: './src/db/schema.ts',
      out: './drizzle/sqlite',
      dialect: 'sqlite',
      dbCredentials: { url: databaseUrl },
    })
