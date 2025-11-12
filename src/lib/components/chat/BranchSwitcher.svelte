<script lang="ts">
	import { GitBranch, Plus } from 'lucide-svelte';
	import { branchStore } from '$lib/stores/branches';
	import { branchService } from '$lib/services/branchService';
	import * as Select from '$lib/components/ui/select';
	import { Button } from '$lib/components/ui/button';

	let { conversationId }: { conversationId: string } = $props();

	let branches = $derived($branchStore.branches);
	let currentBranchId = $derived($branchStore.currentBranchId);

	const currentBranch = $derived(branches.find((b) => b.id === currentBranchId));

	function selectBranch(branchId: string | undefined) {
		if (branchId) {
			branchStore.setCurrentBranch(branchId);
		}
	}

	async function createNewBranch() {
		try {
			const branchName = await branchService.generateBranchName(conversationId);
			const newBranch = await branchService.createBranch(conversationId, branchName);
			branchStore.addBranch(newBranch);
			branchStore.setCurrentBranch(newBranch.id);
		} catch (error) {
			console.error('Failed to create branch:', error);
		}
	}
</script>

<div class="flex items-center gap-1">
	<Select.Root
		selected={{
			value: currentBranchId ?? '',
			label: currentBranch?.name ?? 'No branch'
		}}
		onSelectedChange={(v) => {
			if (v) {
				selectBranch(v.value);
			}
		}}
	>
		<Select.Trigger class="min-w-[120px] w-fit glass-badge hover:glass-light transition-all">
			<div class="flex items-center gap-2">
				<GitBranch class="size-4 text-primary" />
				<span class="truncate max-w-[100px]">{currentBranch?.name ?? 'No branch'}</span>
				{#if branches.length > 0}
					<span class="branch-count">{branches.length}</span>
				{/if}
			</div>
		</Select.Trigger>
		<Select.Portal>
			<Select.Content class="w-[250px]">
				<Select.ScrollUpButton />
				<Select.Viewport class="max-h-[300px]">
					{#if branches.length === 0}
						<div class="empty-state">
							<p class="text-sm text-muted-foreground mb-2">No branches yet</p>
						</div>
					{:else}
						{#each branches as branch (branch.id)}
							<Select.Item value={branch.id} class="branch-item">
								<div class="flex items-center gap-2">
									<GitBranch class="size-3" />
									<span class="flex-1">{branch.name}</span>
									{#if branch.id === currentBranchId}
										<span class="text-primary text-xs">✓</span>
									{/if}
								</div>
							</Select.Item>
						{/each}
					{/if}
				</Select.Viewport>
				<Select.ScrollDownButton />
			</Select.Content>
		</Select.Portal>
	</Select.Root>

	<!-- Create New Branch Button -->
	<Button
		variant="ghost"
		size="icon"
		class="shrink-0 h-8 w-8"
		onclick={createNewBranch}
		title="Create new branch"
	>
		<Plus class="size-4" />
	</Button>
</div>

<style>
	.branch-count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.25rem;
		height: 1.25rem;
		padding: 0 0.25rem;
		border-radius: 0.25rem;
		font-size: 0.7rem;
		font-weight: 600;
		background: rgba(82, 183, 136, 0.2);
		color: #52b788;
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 1rem;
		text-align: center;
	}

	:global(.branch-item) {
		cursor: pointer;
	}
</style>
