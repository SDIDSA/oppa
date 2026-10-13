// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

use super::*;

/// Effect handle; runs immediately on creation, then re-runs in EFFECTS
/// when dirtied (§9.1). `!Send` like every reactive type (locked #20).
///
/// ```compile_fail
/// use oppa::Effect;
/// fn assert_send<T: Send>() {}
/// assert_send::<Effect>();
/// ```
pub struct Effect {
    pub(crate) id: GenerationalId,
    pub(crate) _not_send: PhantomData<Rc<()>>,
}

impl Clone for Effect {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            _not_send: PhantomData,
        }
    }
}

impl Effect {
    /// The effect's generational id (props-update scheduling).
    pub fn id(&self) -> GenerationalId {
        self.id
    }
}
