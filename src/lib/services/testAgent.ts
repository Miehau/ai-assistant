import { agentService } from './agent';
import './chat'; // Registers tools

async function testAgent() {
	console.log('='.repeat(60));
	console.log('Testing Agent Tool Use System');
	console.log('='.repeat(60));

	// Test 1: Simple math query (should use respond tool directly)
	console.log('\n📝 Test 1: Simple math query');
	console.log('Query: "What is 2 + 2?"');
	const result1 = await agentService.processQuery('What is 2 + 2?', 'test-conversation-1');
	console.log('✅ Result:', JSON.stringify(result1, null, 2));

	console.log('\n' + '='.repeat(60));

	// Test 2: Database search query (should use db_search tool)
	console.log('\n📝 Test 2: Database search for clouds');
	console.log('Query: "Search the database for clouds"');
	const result2 = await agentService.processQuery(
		'Search the database for clouds',
		'test-conversation-2'
	);
	console.log('✅ Result:', JSON.stringify(result2, null, 2));

	console.log('\n' + '='.repeat(60));
	console.log('✨ All tests completed!');
}

testAgent().catch(console.error);
