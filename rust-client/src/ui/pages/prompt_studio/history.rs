use xrtranslate_prompt::PromptTemplateProfile;

/// Manages undo and redo stacks for prompt studio graph drafts.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptStudioHistory {
    undo_stack: Vec<PromptTemplateProfile>,
    redo_stack: Vec<PromptTemplateProfile>,
    max_depth: usize,
}

impl Default for PromptStudioHistory {
    fn default() -> Self {
        Self::new(60)
    }
}

impl PromptStudioHistory {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: max_depth.max(1),
        }
    }

    /// Pushes a snapshot of the profile before mutation.
    pub fn push(&mut self, before: PromptTemplateProfile) {
        if before.read_only {
            return;
        }
        if let Some(last) = self.undo_stack.last() {
            if last == &before {
                return;
            }
        }
        self.undo_stack.push(before);
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Undoes the last mutation and pushes the current state to the redo stack.
    pub fn undo(&mut self, current: PromptTemplateProfile) -> Option<PromptTemplateProfile> {
        if current.read_only || self.undo_stack.is_empty() {
            return None;
        }
        let previous = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(previous)
    }

    /// Redoes the undone mutation and pushes the current state to the undo stack.
    pub fn redo(&mut self, current: PromptTemplateProfile) -> Option<PromptTemplateProfile> {
        if current.read_only || self.redo_stack.is_empty() {
            return None;
        }
        let next = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(next)
    }

    pub fn can_undo(&self, read_only: bool) -> bool {
        !read_only && !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self, read_only: bool) -> bool {
        !read_only && !self.redo_stack.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    #[cfg(test)]
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    #[cfg(test)]
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xrtranslate_prompt::{PromptNodeGraph, PromptNodePage, PromptVariable};

    fn make_test_profile(name: &str, read_only: bool) -> PromptTemplateProfile {
        PromptTemplateProfile {
            id: format!("test-{}", name),
            name: name.to_string(),
            description: String::new(),
            graph: PromptNodeGraph::empty(),
            read_only,
        }
    }

    #[test]
    fn history_pushes_and_undo_redo_cycles() {
        let mut history = PromptStudioHistory::default();
        let p1 = make_test_profile("Step 1", false);
        let mut p2 = make_test_profile("Step 2", false);
        p2.graph.add_variable(
            PromptNodePage::OpenAiCompatible,
            PromptVariable::CurrentInput,
            [10.0, 20.0],
        );
        let mut p3 = p2.clone();
        p3.name = "Step 3".to_string();

        assert!(!history.can_undo(false));
        assert!(!history.can_redo(false));

        // Mutation 1 -> 2
        history.push(p1.clone());
        assert!(history.can_undo(false));
        assert_eq!(history.undo_count(), 1);

        // Mutation 2 -> 3
        history.push(p2.clone());
        assert_eq!(history.undo_count(), 2);

        // Undo from 3 -> should restore 2
        let restored_p2 = history.undo(p3.clone()).expect("Undo should succeed");
        assert_eq!(restored_p2, p2);
        assert!(history.can_redo(false));
        assert_eq!(history.redo_count(), 1);

        // Undo from 2 -> should restore 1
        let restored_p1 = history.undo(restored_p2.clone()).expect("Undo should succeed");
        assert_eq!(restored_p1, p1);
        assert!(!history.can_undo(false));
        assert_eq!(history.redo_count(), 2);

        // Redo from 1 -> should restore 2
        let redone_p2 = history.redo(restored_p1).expect("Redo should succeed");
        assert_eq!(redone_p2, p2);
        assert!(history.can_undo(false));
        assert_eq!(history.redo_count(), 1);

        // Redo from 2 -> should restore 3
        let redone_p3 = history.redo(redone_p2).expect("Redo should succeed");
        assert_eq!(redone_p3, p3);
        assert!(!history.can_redo(false));
    }

    #[test]
    fn read_only_profiles_are_not_tracked_or_undone() {
        let mut history = PromptStudioHistory::default();
        let ro = make_test_profile("Locked", true);

        history.push(ro.clone());
        assert!(!history.can_undo(true));
        assert_eq!(history.undo_count(), 0);
        assert!(history.undo(ro).is_none());
    }

    #[test]
    fn new_mutation_clears_redo_stack() {
        let mut history = PromptStudioHistory::default();
        let p1 = make_test_profile("Step 1", false);
        let p2 = make_test_profile("Step 2", false);
        let p3 = make_test_profile("Step 3", false);
        let p4 = make_test_profile("Step 4 Branch", false);

        history.push(p1.clone());
        history.push(p2.clone());

        // Undo to p2
        let _ = history.undo(p3.clone());
        assert_eq!(history.redo_count(), 1);

        // Branch by pushing p4
        history.push(p4);
        assert_eq!(history.redo_count(), 0);
        assert!(!history.can_redo(false));
    }

    #[test]
    fn max_depth_bounds_history_capacity() {
        let mut history = PromptStudioHistory::new(3);
        for i in 0..10 {
            history.push(make_test_profile(&format!("Step {i}"), false));
        }
        assert_eq!(history.undo_count(), 3);
    }
}
