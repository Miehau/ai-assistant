<script lang="ts">
	import { X, Network } from 'lucide-svelte';
	import { fly, fade } from 'svelte/transition';
	import BranchTreeView from './BranchTreeView.svelte';
	import BranchIndicator from '../chat/BranchIndicator.svelte';
	import { branchStore } from '$lib/stores/branches';

	let { conversationId, open, onClose }: { conversationId: string; open: boolean; onClose: () => void } = $props();

	let branches = $derived($branchStore.branches);
	let currentBranchId = $derived($branchStore.currentBranchId);
	let tree = $derived($branchStore.tree);

	const currentBranch = $derived(branches.find((b) => b.id === currentBranchId));
</script>

{#if open}
	<!-- Overlay -->
	<div
		class="drawer-overlay"
		onclick={onClose}
		transition:fade={{ duration: 200 }}
		role="button"
		tabindex="0"
		onkeydown={(e) => e.key === 'Escape' && onClose()}
	></div>

	<!-- Drawer -->
	<div class="branch-drawer" transition:fly={{ x: 400, duration: 300 }}>
		<!-- Header -->
		<div class="drawer-header">
			<div class="header-left">
				<span class="header-icon">
					<Network size={20} />
				</span>
				<h2 class="drawer-title">Branch Tree</h2>
			</div>
			<button onclick={onClose} class="close-button" title="Close">
				<X size={20} />
			</button>
		</div>

		<!-- Current Branch Info -->
		{#if currentBranch}
			<div class="current-branch-info">
				<div class="info-label">Current Branch</div>
				<BranchIndicator branch={currentBranch} />
			</div>
		{/if}

		<!-- Branch Stats -->
		{#if tree}
			<div class="branch-stats">
				<div class="stat-item">
					<span class="stat-label">Branches</span>
					<span class="stat-value">{branches.length}</span>
				</div>
				<div class="stat-item">
					<span class="stat-label">Messages</span>
					<span class="stat-value">{tree.messages.length}</span>
				</div>
				<div class="stat-item">
					<span class="stat-label">Branch Points</span>
					<span class="stat-value">{tree.nodes.filter((n) => n.branch_point).length}</span>
				</div>
			</div>
		{/if}

		<!-- Tree View -->
		<div class="tree-container">
			<BranchTreeView {conversationId} />
		</div>
	</div>
{/if}

<style>
	.drawer-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.45);
		backdrop-filter: blur(6px);
		z-index: 40;
	}

	.branch-drawer {
		position: fixed;
		top: 0;
		right: 0;
		bottom: 0;
		width: 100%;
		max-width: 50rem;
		background: rgba(14, 14, 16, 0.92);
		backdrop-filter: blur(18px) saturate(140%);
		border-left: 1px solid rgba(255, 255, 255, 0.08);
		box-shadow: -12px 0 40px rgba(0, 0, 0, 0.55);
		z-index: 50;
		display: flex;
		flex-direction: column;
	}

	.drawer-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1.25rem 1.5rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.08);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.header-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: rgba(255, 255, 255, 0.7);
	}

	.drawer-title {
		font-size: 1.25rem;
		font-weight: 600;
		color: rgba(255, 255, 255, 0.95);
		margin: 0;
	}

	.close-button {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.375rem;
		background: rgba(255, 255, 255, 0.04);
		border: 1px solid rgba(255, 255, 255, 0.08);
		color: rgba(255, 255, 255, 0.8);
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.close-button:hover {
		background: rgba(255, 255, 255, 0.1);
		color: rgba(255, 255, 255, 0.9);
	}

	.current-branch-info {
		padding: 1rem 1.5rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.08);
	}

	.info-label {
		font-size: 0.75rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: rgba(255, 255, 255, 0.5);
		margin-bottom: 0.5rem;
	}

	.branch-stats {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 1rem;
		padding: 1rem 1.5rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.08);
	}

	.stat-item {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 0.75rem;
		border-radius: 0.375rem;
		background: rgba(255, 255, 255, 0.025);
		border: 1px solid rgba(255, 255, 255, 0.06);
	}

	.stat-label {
		font-size: 0.75rem;
		color: rgba(255, 255, 255, 0.5);
	}

	.stat-value {
		font-size: 1.5rem;
		font-weight: 700;
		color: rgba(255, 255, 255, 0.9);
	}

	.tree-container {
		flex: 1;
		overflow: hidden;
		padding: 1.5rem;
	}
</style>
