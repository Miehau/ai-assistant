<script lang="ts">
	import { onMount } from 'svelte';
	import { Plus, X } from 'lucide-svelte';
	import ComparisonColumn from './ComparisonColumn.svelte';
	import { branchService } from '$lib/services/branchService';
	import { branchStore } from '$lib/stores/branches';
	import type { BranchPath } from '$lib/types';

	let { conversationId }: { conversationId: string } = $props();

	let branches = $derived($branchStore.branches);
	let selectedBranchIds = $state<string[]>([]);
	let branchPaths = $state<(BranchPath | null)[]>([null, null]);
	let loading = $state(false);

	// Load branch paths when selected branches change
	$effect(() => {
		loadBranchPaths();
	});

	async function loadBranchPaths() {
		loading = true;
		try {
			const paths = await Promise.all(
				selectedBranchIds.map(async (branchId) => {
					if (branchId) {
						return await branchService.getBranchPath(branchId);
					}
					return null;
				})
			);
			branchPaths = paths;
		} catch (error) {
			console.error('Failed to load branch paths:', error);
		} finally {
			loading = false;
		}
	}

	function selectBranch(index: number, branchId: string) {
		const newIds = [...selectedBranchIds];
		newIds[index] = branchId;
		selectedBranchIds = newIds;
	}

	function addColumn() {
		selectedBranchIds = [...selectedBranchIds, ''];
		branchPaths = [...branchPaths, null];
	}

	function removeColumn(index: number) {
		selectedBranchIds = selectedBranchIds.filter((_, i) => i !== index);
		branchPaths = branchPaths.filter((_, i) => i !== index);
	}

	onMount(() => {
		// Pre-select first two branches if available
		if (branches.length >= 2) {
			selectedBranchIds = [branches[0].id, branches[1].id];
		} else if (branches.length === 1) {
			selectedBranchIds = [branches[0].id, ''];
		}
	});
</script>

<div class="branch-comparison">
	<!-- Header -->
	<div class="comparison-header">
		<h2 class="comparison-title">Branch Comparison</h2>
		<button
			onclick={addColumn}
			class="add-column-btn glass-badge"
			disabled={selectedBranchIds.length >= 4}
			title="Add comparison column (max 4)"
		>
			<Plus size={16} />
			Add Column
		</button>
	</div>

	<!-- Column Selectors -->
	<div class="column-selectors">
		{#each selectedBranchIds as branchId, index}
			<div class="selector-group">
				<select
					bind:value={selectedBranchIds[index]}
					onchange={() => selectBranch(index, selectedBranchIds[index])}
					class="branch-select glass-panel"
				>
					<option value="">Select branch...</option>
					{#each branches as branch}
						<option value={branch.id}>{branch.name}</option>
					{/each}
				</select>

				{#if selectedBranchIds.length > 2}
					<button
						onclick={() => removeColumn(index)}
						class="remove-column-btn"
						title="Remove column"
					>
						<X size={16} />
					</button>
				{/if}
			</div>
		{/each}
	</div>

	<!-- Comparison Grid -->
	<div class="comparison-grid" style="--column-count: {branchPaths.length}">
		{#each branchPaths as branchPath, index}
			<ComparisonColumn {branchPath} />
		{/each}
	</div>

	{#if loading}
		<div class="loading-overlay glass-panel">
			<div class="loading-spinner"></div>
			<p>Loading branches...</p>
		</div>
	{/if}
</div>

<style>
	.branch-comparison {
		display: flex;
		flex-direction: column;
		height: 100%;
		gap: 1rem;
	}

	.comparison-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.1);
	}

	.comparison-title {
		font-size: 1.25rem;
		font-weight: 600;
		color: rgba(255, 255, 255, 0.9);
		margin: 0;
	}

	.add-column-btn {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
		border-radius: 0.375rem;
		background: rgba(82, 183, 136, 0.2);
		border: 1px solid rgba(82, 183, 136, 0.3);
		color: #52b788;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.add-column-btn:hover:not(:disabled) {
		background: rgba(82, 183, 136, 0.3);
		box-shadow: 0 0 15px rgba(82, 183, 136, 0.3);
	}

	.add-column-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.column-selectors {
		display: grid;
		grid-template-columns: repeat(var(--column-count, 2), 1fr);
		gap: 1rem;
		padding: 0 1rem;
	}

	.selector-group {
		display: flex;
		gap: 0.5rem;
	}

	.branch-select {
		flex: 1;
		padding: 0.75rem;
		border-radius: 0.375rem;
		background: rgba(255, 255, 255, 0.05);
		backdrop-filter: blur(20px);
		border: 1px solid rgba(255, 255, 255, 0.1);
		color: rgba(255, 255, 255, 0.9);
		font-size: 0.875rem;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.branch-select:hover {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(82, 183, 136, 0.3);
	}

	.branch-select:focus {
		outline: none;
		border-color: rgba(82, 183, 136, 0.5);
		box-shadow: 0 0 15px rgba(82, 183, 136, 0.2);
	}

	.branch-select option {
		background: #1a1a1a;
		color: rgba(255, 255, 255, 0.9);
	}

	.remove-column-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.5rem;
		height: 2.5rem;
		border-radius: 0.375rem;
		background: rgba(239, 68, 68, 0.1);
		border: 1px solid rgba(239, 68, 68, 0.3);
		color: #ef4444;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.remove-column-btn:hover {
		background: rgba(239, 68, 68, 0.2);
		box-shadow: 0 0 10px rgba(239, 68, 68, 0.3);
	}

	.comparison-grid {
		flex: 1;
		display: grid;
		grid-template-columns: repeat(var(--column-count, 2), 1fr);
		gap: 1rem;
		padding: 0 1rem 1rem;
		overflow: hidden;
	}

	.loading-overlay {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		background: rgba(0, 0, 0, 0.8);
		backdrop-filter: blur(10px);
		z-index: 10;
	}

	.loading-spinner {
		width: 2rem;
		height: 2rem;
		border: 3px solid rgba(82, 183, 136, 0.2);
		border-top-color: #52b788;
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	.loading-overlay p {
		font-size: 0.875rem;
		color: rgba(255, 255, 255, 0.7);
	}
</style>
