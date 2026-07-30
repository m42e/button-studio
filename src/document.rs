use crate::model::Design;
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone)]
pub struct SharedDocument {
    design: Arc<Mutex<Design>>,
    revision: Arc<AtomicU64>,
}

impl SharedDocument {
    pub fn new(design: Design) -> Self {
        Self {
            design: Arc::new(Mutex::new(design)),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, Design>, String> {
        self.design
            .lock()
            .map_err(|_| "Design state lock is poisoned".to_owned())
    }

    pub fn snapshot(&self) -> Result<(Design, u64), String> {
        let design = self.lock()?.clone();
        Ok((design, self.revision.load(Ordering::Acquire)))
    }

    pub fn mark_changed(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn replace_if_revision(&self, expected_revision: u64, design: &Design) -> Result<u64, u64> {
        let Ok(mut shared_design) = self.design.lock() else {
            return Err(self.revision.load(Ordering::Acquire));
        };
        let current_revision = self.revision.load(Ordering::Acquire);
        if current_revision != expected_revision {
            return Err(current_revision);
        }
        if *shared_design == *design {
            return Ok(current_revision);
        }
        *shared_design = design.clone();
        Ok(self.mark_changed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_gui_revision_cannot_overwrite_a_newer_document() {
        let document = SharedDocument::new(Design::default());
        let (mut stale_design, revision) = document.snapshot().unwrap();
        document.lock().unwrap().add_text("MCP".to_owned());
        document.mark_changed();

        stale_design.background.red = 0;
        assert_eq!(
            document.replace_if_revision(revision, &stale_design),
            Err(1)
        );
        assert_eq!(document.snapshot().unwrap().0.elements.len(), 3);
    }
}
