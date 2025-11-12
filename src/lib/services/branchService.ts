import { invoke } from '@tauri-apps/api/tauri';
import type {
	Branch,
	MessageTreeNode,
	ConversationTree,
	BranchPath,
	BranchStats
} from '$lib/types';

/**
 * Service for managing conversation branches
 * Wraps Tauri commands for branch operations
 */
export class BranchService {
	/**
	 * Create a new branch in a conversation
	 */
	async createBranch(conversationId: string, name: string): Promise<Branch> {
		return await invoke<Branch>('create_branch', {
			conversationId,
			name
		});
	}

	/**
	 * Create a message tree node linking a message to its parent and branch
	 */
	async createMessageTreeNode(
		messageId: string,
		parentMessageId: string | null,
		branchId: string,
		isBranchPoint: boolean
	): Promise<MessageTreeNode> {
		return await invoke<MessageTreeNode>('create_message_tree_node', {
			messageId,
			parentMessageId,
			branchId,
			isBranchPoint
		});
	}

	/**
	 * Get all branches for a conversation
	 */
	async getConversationBranches(conversationId: string): Promise<Branch[]> {
		return await invoke<Branch[]>('get_conversation_branches', {
			conversationId
		});
	}

	/**
	 * Get the complete conversation tree structure
	 */
	async getConversationTree(conversationId: string): Promise<ConversationTree> {
		return await invoke<ConversationTree>('get_conversation_tree', {
			conversationId
		});
	}

	/**
	 * Get a specific branch path with its messages
	 */
	async getBranchPath(branchId: string): Promise<BranchPath> {
		return await invoke<BranchPath>('get_branch_path', {
			branchId
		});
	}

	/**
	 * Rename a branch
	 */
	async renameBranch(branchId: string, newName: string): Promise<void> {
		await invoke('rename_branch', {
			branchId,
			newName
		});
	}

	/**
	 * Delete a branch
	 */
	async deleteBranch(branchId: string): Promise<void> {
		await invoke('delete_branch', {
			branchId
		});
	}

	/**
	 * Get branch statistics for a conversation
	 */
	async getBranchStats(conversationId: string): Promise<BranchStats> {
		return await invoke<BranchStats>('get_branch_stats', {
			conversationId
		});
	}

	/**
	 * Get or create the main branch for a conversation
	 */
	async getOrCreateMainBranch(conversationId: string): Promise<Branch> {
		return await invoke<Branch>('get_or_create_main_branch', {
			conversationId
		});
	}

	/**
	 * Create a new branch from a specific message in the conversation
	 */
	async createBranchFromMessage(
		conversationId: string,
		parentMessageId: string,
		branchName: string
	): Promise<Branch> {
		return await invoke<Branch>('create_branch_from_message', {
			conversationId,
			parentMessageId,
			branchName
		});
	}

	/**
	 * Generate an auto-incrementing branch name
	 */
	async generateBranchName(conversationId: string): Promise<string> {
		const branches = await this.getConversationBranches(conversationId);
		const branchNumbers = branches
			.map((b) => {
				const match = b.name.match(/^Branch (\d+)$/);
				return match ? parseInt(match[1]) : 0;
			})
			.filter((n) => n > 0);

		const nextNumber = branchNumbers.length > 0 ? Math.max(...branchNumbers) + 1 : 1;
		return `Branch ${nextNumber}`;
	}

	/**
	 * Check message tree consistency and identify orphaned messages
	 */
	async checkMessageTreeConsistency(): Promise<any> {
		return await invoke('check_message_tree_consistency');
	}

	/**
	 * Repair message tree by adding orphaned messages to their conversation's main branch
	 */
	async repairMessageTree(): Promise<number> {
		return await invoke<number>('repair_message_tree');
	}
}

// Singleton instance
export const branchService = new BranchService();
