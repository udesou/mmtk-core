use crate::util::metadata::HashedKind;
use crate::util::metadata::{HASHED, HASHED_MOVED};
use crate::util::ObjectReference;
use crate::vm::VMBinding;
use crate::vm::VMLocalAddrspaceHashingBitSpec;
use std::sync::atomic::Ordering;

impl VMLocalAddrspaceHashingBitSpec {
    pub fn mark_hashed<VM: VMBinding>(&self, object: ObjectReference) {
        self.store_atomic::<VM, u8>(object, HASHED, None, Ordering::SeqCst);
    }

    pub fn mark_hashed_moved<VM: VMBinding>(&self, object: ObjectReference) {
        self.store_atomic::<VM, u8>(object, HASHED_MOVED, None, Ordering::SeqCst);
    }

    pub fn check_hashing_status<VM: VMBinding>(&self, object: ObjectReference) -> HashedKind {
        self.load_atomic::<VM, u8>(object, None, Ordering::SeqCst)
    }
}
