<script lang="ts">
	import type { Branch, BranchPath } from '$lib/types';
	import BranchIndicator from '../chat/BranchIndicator.svelte';
	import ChatMessage from '../ChatMessage.svelte';

	let { branchPath }: { branchPath: BranchPath | null } = $props();

	const messages = $derived(
		branchPath?.messages.map((msg) => ({
			type: msg.role === 'user' ? 'sent' : 'received',
			content: msg.content,
			attachments: msg.attachments
		})) ?? []
	);
</script>

<div class="comparison-column glass-panel">
	{#if branchPath}
		<!-- Column Header -->
		<div class="column-header">
			<BranchIndicator branch={branchPath.branch} />
			<div class="message-count">
				{branchPath.messages.length} {branchPath.messages.length === 1 ? 'message' : 'messages'}
			</div>
		</div>

		<!-- Messages -->
		<div class="messages-container">
			{#each messages as message, index (index)}
				<div class="message-wrapper">
					<ChatMessage
						type={message.type}
						content={message.content}
						attachments={message.attachments}
					/>
				</div>
			{/each}

			{#if messages.length === 0}
				<div class="empty-messages">
					<p>No messages in this branch</p>
				</div>
			{/if}
		</div>
	{:else}
		<div class="empty-column">
			<p>Select a branch to compare</p>
		</div>
	{/if}
</div>

<style>
	.comparison-column {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: rgba(255, 255, 255, 0.02);
		backdrop-filter: blur(20px);
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 0.5rem;
		overflow: hidden;
	}

	.column-header {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 1rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.1);
		background: rgba(255, 255, 255, 0.03);
	}

	.message-count {
		font-size: 0.75rem;
		color: rgba(255, 255, 255, 0.5);
	}

	.messages-container {
		flex: 1;
		overflow-y: auto;
		padding: 1rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.message-wrapper {
		animation: fadeIn 0.3s ease;
	}

	@keyframes fadeIn {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	.empty-messages,
	.empty-column {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 3rem;
		text-align: center;
	}

	.empty-messages p,
	.empty-column p {
		font-size: 0.875rem;
		color: rgba(255, 255, 255, 0.5);
	}
</style>
