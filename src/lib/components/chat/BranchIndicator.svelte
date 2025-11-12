<script lang="ts">
	import { GitBranch } from 'lucide-svelte';
	import type { Branch } from '$lib/types';

	let { branch, compact = false }: { branch: Branch; compact?: boolean } = $props();

	// Format date
	const formatDate = (dateStr: string) => {
		const date = new Date(dateStr);
		const now = new Date();
		const diff = now.getTime() - date.getTime();
		const days = Math.floor(diff / (1000 * 60 * 60 * 24));

		if (days === 0) return 'today';
		if (days === 1) return 'yesterday';
		if (days < 7) return `${days}d ago`;
		if (days < 30) return `${Math.floor(days / 7)}w ago`;
		return `${Math.floor(days / 30)}mo ago`;
	};
</script>

{#if compact}
	<div class="branch-indicator-compact glass-badge">
		<GitBranch size={12} />
		<span class="branch-name">{branch.name}</span>
	</div>
{:else}
	<div class="branch-indicator glass-panel">
		<div class="branch-icon">
			<GitBranch size={16} />
		</div>
		<div class="branch-info">
			<div class="branch-name">{branch.name}</div>
			<div class="branch-meta">{formatDate(branch.created_at)}</div>
		</div>
	</div>
{/if}

<style>
	.branch-indicator-compact {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0.5rem;
		font-size: 0.75rem;
		border-radius: 0.375rem;
		background: rgba(82, 183, 136, 0.1);
		border: 1px solid rgba(82, 183, 136, 0.3);
		color: #52b788;
	}

	.branch-indicator {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem;
		border-radius: 0.5rem;
		background: rgba(255, 255, 255, 0.05);
		backdrop-filter: blur(20px);
		border: 1px solid rgba(255, 255, 255, 0.1);
		transition: all 0.2s ease;
	}

	.branch-indicator:hover {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(82, 183, 136, 0.3);
		box-shadow: 0 0 15px rgba(82, 183, 136, 0.2);
	}

	.branch-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.375rem;
		background: linear-gradient(135deg, rgba(82, 183, 136, 0.2), rgba(6, 255, 165, 0.2));
		color: #52b788;
	}

	.branch-info {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.branch-name {
		font-weight: 600;
		font-size: 0.875rem;
		color: rgba(255, 255, 255, 0.9);
	}

	.branch-indicator-compact .branch-name {
		font-size: 0.75rem;
		font-weight: 500;
	}

	.branch-meta {
		font-size: 0.75rem;
		color: rgba(255, 255, 255, 0.5);
	}
</style>
