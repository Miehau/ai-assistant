<script lang="ts">
	import { GitBranch } from 'lucide-svelte';
	import { branchService } from '$lib/services/branchService';
	import { branchStore } from '$lib/stores/branches';

	let {
		messageId,
		conversationId,
		hasBranches = false,
		branchCount = 0
	}: {
		messageId: string;
		conversationId: string;
		hasBranches?: boolean;
		branchCount?: number;
	} = $props();

	let isCreating = false;

	async function createBranch() {
		if (isCreating) return;

		try {
			isCreating = true;

			// Generate branch name
			const branchName = await branchService.generateBranchName(conversationId);

			// Create the branch
			const newBranch = await branchService.createBranchFromMessage(
				conversationId,
				messageId,
				branchName
			);

			// Add to store
			branchStore.addBranch(newBranch);

			console.log('Created branch:', newBranch);
		} catch (error: any) {
			console.error('Failed to create branch:', error);
			isCreating = false;

			// Show user-friendly error message
			const errorMessage = error?.message || String(error);

			if (errorMessage.includes('not in the message tree')) {
				// Offer to repair the database
				const repair = confirm(
					'This message appears to be missing from the conversation tree. ' +
					'This can happen with older conversations.\n\n' +
					'Would you like to try repairing the conversation tree? ' +
					'This will attempt to restore the missing connections.'
				);

				if (repair) {
					try {
						const repairedCount = await branchService.repairMessageTree();
						alert(
							`Successfully repaired ${repairedCount} message${repairedCount === 1 ? '' : 's'}. ` +
							'Please try creating the branch again.'
						);
					} catch (repairError) {
						console.error('Failed to repair message tree:', repairError);
						alert('Failed to repair the conversation tree. Please try restarting the application.');
					}
				}
			} else if (errorMessage.includes('Message not found')) {
				alert(
					'This message could not be found in the database. ' +
					'This may indicate a data consistency issue. ' +
					'Please try restarting the application.'
				);
			} else if (errorMessage.includes('Conversation not found')) {
				alert('The conversation could not be found. Please refresh the page.');
			} else {
				// Generic error message
				alert(
					`Failed to create branch: ${errorMessage}\n\n` +
					'If this problem persists, please restart the application.'
				);
			}
		} finally {
			isCreating = false;
		}
	}
</script>

<button
	onclick={createBranch}
	disabled={isCreating}
	class="branch-button glass-badge transition-all duration-200 hover:scale-105"
	class:has-branches={hasBranches}
	class:creating={isCreating}
	title={hasBranches
		? `${branchCount} ${branchCount === 1 ? 'branch' : 'branches'} from this message`
		: 'Create branch from this message'}
>
	<span class:spin={isCreating}>
		<GitBranch size={14} class="inline" />
	</span>
	{#if hasBranches}
		<span class="branch-count">{branchCount}</span>
	{:else if isCreating}
		<span class="creating-text">Creating...</span>
	{/if}
</button>

<style>
	.branch-button {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.25rem 0.5rem;
		font-size: 0.75rem;
		border-radius: 0.375rem;
		background: rgba(255, 255, 255, 0.05);
		backdrop-filter: blur(10px);
		border: 1px solid rgba(255, 255, 255, 0.1);
		color: rgba(255, 255, 255, 0.7);
		cursor: pointer;
	}

	.branch-button:hover {
		background: rgba(255, 255, 255, 0.1);
		border-color: rgba(82, 183, 136, 0.5);
		box-shadow: 0 0 10px rgba(82, 183, 136, 0.3);
		color: rgba(255, 255, 255, 0.9);
	}

	.branch-button.has-branches {
		background: linear-gradient(135deg, rgba(82, 183, 136, 0.2), rgba(6, 255, 165, 0.2));
		border-color: rgba(82, 183, 136, 0.4);
		color: #52b788;
	}

	.branch-button.has-branches:hover {
		box-shadow: 0 0 15px rgba(82, 183, 136, 0.5);
	}

	.branch-count {
		font-weight: 600;
		font-size: 0.7rem;
	}

	.branch-button.creating {
		opacity: 0.7;
		cursor: wait;
	}

	.creating-text {
		font-size: 0.7rem;
		margin-left: 0.25rem;
	}

	.spin {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
