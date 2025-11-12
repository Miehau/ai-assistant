<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import BranchComparison from '$lib/components/branch/BranchComparison.svelte';
	import { branchService } from '$lib/services/branchService';
	import { branchStore } from '$lib/stores/branches';
	import { ArrowLeft } from 'lucide-svelte';

	// Get conversation ID from query params
	const conversationId = $derived($page.url.searchParams.get('conversation'));

	onMount(async () => {
		if (!conversationId) {
			// Redirect to home if no conversation ID
			goto('/');
			return;
		}

		// Load conversation branches and tree
		try {
			const branches = await branchService.getConversationBranches(conversationId);
			const tree = await branchService.getConversationTree(conversationId);
			branchStore.loadConversation(branches, tree);
		} catch (error) {
			console.error('Failed to load branches:', error);
		}
	});

	function goBack() {
		goto('/');
	}
</script>

<svelte:head>
	<title>Branch Comparison - AI Frontend</title>
</svelte:head>

<div class="compare-page">
	<!-- Header -->
	<div class="page-header glass-panel">
		<button onclick={goBack} class="back-button" title="Back to chat">
			<ArrowLeft size={20} />
			<span>Back to Chat</span>
		</button>
	</div>

	<!-- Main Content -->
	<div class="page-content">
		{#if conversationId}
			<BranchComparison {conversationId} />
		{:else}
			<div class="error-state">
				<p>No conversation selected</p>
				<button onclick={goBack} class="go-home-btn">Go to Home</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.compare-page {
		display: flex;
		flex-direction: column;
		height: 100vh;
		background: linear-gradient(135deg, rgba(0, 0, 0, 0.9) 0%, rgba(26, 26, 26, 0.9) 100%);
	}

	.page-header {
		padding: 1rem 1.5rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.1);
		background: rgba(255, 255, 255, 0.02);
		backdrop-filter: blur(20px);
	}

	.back-button {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
		border-radius: 0.375rem;
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.1);
		color: rgba(255, 255, 255, 0.9);
		font-size: 0.875rem;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.back-button:hover {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(82, 183, 136, 0.3);
		box-shadow: 0 0 15px rgba(82, 183, 136, 0.2);
	}

	.page-content {
		flex: 1;
		overflow: hidden;
		padding: 1rem;
	}

	.error-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		gap: 1rem;
	}

	.error-state p {
		font-size: 1rem;
		color: rgba(255, 255, 255, 0.6);
	}

	.go-home-btn {
		padding: 0.75rem 1.5rem;
		border-radius: 0.375rem;
		background: linear-gradient(135deg, rgba(82, 183, 136, 0.2), rgba(6, 255, 165, 0.2));
		border: 1px solid rgba(82, 183, 136, 0.3);
		color: #52b788;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.go-home-btn:hover {
		background: linear-gradient(135deg, rgba(82, 183, 136, 0.3), rgba(6, 255, 165, 0.3));
		box-shadow: 0 0 15px rgba(82, 183, 136, 0.3);
	}
</style>
