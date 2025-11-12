import { writable } from 'svelte/store';
import type { Branch, ConversationTree, BranchState } from '$lib/types';

/**
 * Initial branch state
 */
const initialState: BranchState = {
	currentBranchId: null,
	branches: [],
	tree: null,
	selectedPath: []
};

/**
 * Branch state store
 */
function createBranchStore() {
	const { subscribe, set, update } = writable<BranchState>(initialState);

	return {
		subscribe,

		/**
		 * Set the current branch
		 */
		setCurrentBranch: (branchId: string) => {
			update((state) => ({
				...state,
				currentBranchId: branchId
			}));
		},

		/**
		 * Set all branches for a conversation
		 */
		setBranches: (branches: Branch[]) => {
			update((state) => ({
				...state,
				branches
			}));
		},

		/**
		 * Set the conversation tree
		 */
		setTree: (tree: ConversationTree) => {
			update((state) => ({
				...state,
				tree
			}));
		},

		/**
		 * Set the selected path (array of message IDs)
		 */
		setSelectedPath: (path: string[]) => {
			update((state) => ({
				...state,
				selectedPath: path
			}));
		},

		/**
		 * Add a new branch
		 */
		addBranch: (branch: Branch) => {
			update((state) => ({
				...state,
				branches: [...state.branches, branch]
			}));
		},

		/**
		 * Remove a branch
		 */
		removeBranch: (branchId: string) => {
			update((state) => ({
				...state,
				branches: state.branches.filter((b) => b.id !== branchId),
				currentBranchId: state.currentBranchId === branchId ? null : state.currentBranchId
			}));
		},

		/**
		 * Update a branch name
		 */
		updateBranchName: (branchId: string, newName: string) => {
			update((state) => ({
				...state,
				branches: state.branches.map((b) => (b.id === branchId ? { ...b, name: newName } : b))
			}));
		},

		/**
		 * Reset to initial state
		 */
		reset: () => {
			set(initialState);
		},

		/**
		 * Load state for a conversation
		 */
		loadConversation: (branches: Branch[], tree: ConversationTree, currentBranchId?: string) => {
			set({
				currentBranchId: currentBranchId ?? (branches.length > 0 ? branches[0].id : null),
				branches,
				tree,
				selectedPath: []
			});
		}
	};
}

export const branchStore = createBranchStore();
