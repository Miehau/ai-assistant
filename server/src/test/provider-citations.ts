import assert from 'node:assert/strict'
import {
  ProviderCitationStreamSanitizer,
  sanitizeProviderCitations,
  containsProviderCitationArtifacts,
} from '../lib/provider-citations.js'

const privateCitation = 'Claim with provider marker. \uE200cite\uE202turn0search3\uE201'
assert.equal(sanitizeProviderCitations(privateCitation), 'Claim with provider marker.')
assert.equal(containsProviderCitationArtifacts(privateCitation), true)

const placeholderCitation = 'Claim cites turn0search4 and turn12fetch3.'
assert.equal(sanitizeProviderCitations(placeholderCitation), 'Claim cites and.')
assert.equal(containsProviderCitationArtifacts(placeholderCitation), true)

const clean = 'Claim cites https://example.com/source with a real URL.'
assert.equal(sanitizeProviderCitations(clean), clean)
assert.equal(containsProviderCitationArtifacts(clean), false)

const stream = new ProviderCitationStreamSanitizer()
const streamed = [
  stream.push('A streamed claim. \uE200ci'),
  stream.push('te\uE202turn0'),
  stream.push('search3\uE201 Next'),
  stream.push(' source is https://example.com.'),
  stream.flush(),
].join('')
assert.equal(streamed, 'A streamed claim. Next source is https://example.com.')

console.log('Provider citation tests passed')
