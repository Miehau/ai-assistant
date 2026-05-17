import assert from 'node:assert/strict'
import { ToolRegistryImpl } from '../tools/registry.js'
import { registerWebTools } from '../tools/web.js'

const originalFetch = globalThis.fetch
const registry = new ToolRegistryImpl()
registerWebTools(registry)

const ctx = {
  agent_id: 'agent',
  session_id: 'session',
  signal: new AbortController().signal,
}

try {
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input)

    if (url.endsWith('/page')) {
      return new Response(`
        <!doctype html>
        <html>
          <head>
            <title>Token-heavy title</title>
            <meta name="description" content="metadata should not leak">
          </head>
          <body>
            <h1>Visible heading</h1>
            <img src="/large.jpg" alt="image source should not leak">
            <script src="/app.js">console.log('hidden')</script>
            <style>.hidden { display: none; }</style>
            <p>Useful &amp; readable body.</p>
          </body>
        </html>
      `, {
        status: 200,
        headers: {
          'content-type': 'text/html; charset=utf-8',
          'x-noisy-header': 'not useful to the model',
        },
      })
    }

    if (url.endsWith('/raw')) {
      return new Response('<html><head><title>Raw head</title></head><body>Raw body</body></html>', {
        status: 201,
        headers: {
          'content-type': 'text/html',
          'x-debug': 'kept for low-level request',
        },
      })
    }

    if (url.endsWith('/submit')) {
      assert.equal(init?.method, 'POST')
      assert.equal(init?.body, 'name=Ada')
      return new Response('<html><head><title>Posted</title></head><body><p>Saved</p></body></html>', {
        status: 200,
        headers: {
          'content-type': 'text/html',
          'x-form-header': 'should not be returned',
        },
      })
    }

    throw new Error(`Unexpected fetch URL: ${url}`)
  }) as typeof fetch

  const fetched = await registry.execute('web.fetch', { url: 'https://example.test/page' }, ctx)
  assert.equal(fetched.ok, true)
  assert.equal(typeof fetched.output, 'string')
  assert.match(fetched.output as string, /Visible heading/)
  assert.match(fetched.output as string, /Useful & readable body/)
  assert.doesNotMatch(fetched.output as string, /Token-heavy title/)
  assert.doesNotMatch(fetched.output as string, /src=/)
  assert.doesNotMatch(fetched.output as string, /x-noisy-header/)

  const raw = await registry.execute('web.request', {
    method: 'GET',
    url: 'https://example.test/raw',
  }, ctx)
  assert.equal(raw.ok, true)
  assert.equal((raw.output as { status: number }).status, 201)
  assert.equal((raw.output as { headers: Record<string, string> }).headers['x-debug'], 'kept for low-level request')
  assert.match((raw.output as { body: string }).body, /<head>/)

  const posted = await registry.execute('web.post_form', {
    url: 'https://example.test/submit',
    fields: { name: 'Ada' },
  }, ctx)
  assert.equal(posted.ok, true)
  assert.deepEqual(posted.output, { status: 200, body: 'Saved' })

  console.log('Web tool output tests passed')
} finally {
  globalThis.fetch = originalFetch
}
