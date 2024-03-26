use super::*;
use crate::plan::Plan;
use crate::scheduler::gc_work::*;
use crate::util::ObjectReference;
use crate::vm::edge_shape::Edge;
use crate::vm::*;
use crate::MMTK;
use crate::{scheduler::*, ObjectQueue};
use prost::Message;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::Mutex;

#[allow(dead_code)]
pub struct SanityChecker<ES: Edge> {
    /// Visited objects
    refs: HashSet<ObjectReference>,
    /// Cached root edges for sanity root scanning
    root_edges: Vec<Vec<ES>>,
    /// Cached root nodes for sanity root scanning
    root_nodes: Vec<Vec<ObjectReference>>,
    pub(crate) iter: ShapesIteration,
    heapdump: HeapDump,
}

lazy_static! {
    static ref SANITY_SLOTS: Mutex<HashMap<ObjectReference, Shape>> = Mutex::new(HashMap::new());
}

impl HeapDump {
    fn dump_to_file(&self, path: impl AsRef<Path>) {
        let file = File::create(path).unwrap();
        let mut writer = zstd::Encoder::new(file, 0).unwrap().auto_finish();
        let mut buf = Vec::new();
        self.encode(&mut buf).unwrap();
        writer.write_all(&buf).unwrap();
    }

    fn reset(&mut self) {
        self.objects.clear();
        self.roots.clear();
        self.spaces.clear();
    }

    fn new() -> Self {
        HeapDump {
            objects: vec![],
            roots: vec![],
            spaces: vec![],
        }
    }
}

impl<ES: Edge> Default for SanityChecker<ES> {
    fn default() -> Self {
        Self::new()
    }
}

impl<ES: Edge> SanityChecker<ES> {
    pub fn new() -> Self {
        Self {
            refs: HashSet::new(),
            root_edges: vec![],
            root_nodes: vec![],
            iter: ShapesIteration { epochs: vec![] },
            heapdump: HeapDump::new(),
        }
    }

    /// Cache a list of root edges to the sanity checker.
    pub fn add_root_edges(&mut self, roots: Vec<ES>) {
        self.root_edges.push(roots)
    }

    pub fn add_root_nodes(&mut self, roots: Vec<ObjectReference>) {
        self.root_nodes.push(roots)
    }

    /// Reset roots cache at the end of the sanity gc.
    fn clear_roots_cache(&mut self) {
        self.root_edges.clear();
        self.root_nodes.clear();
    }
}

pub struct ScheduleSanityGC<P: Plan> {
    _plan: &'static P,
}

impl<P: Plan> ScheduleSanityGC<P> {
    pub fn new(plan: &'static P) -> Self {
        ScheduleSanityGC { _plan: plan }
    }
}

impl<P: Plan> GCWork<P::VM> for ScheduleSanityGC<P> {
    fn do_work(&mut self, worker: &mut GCWorker<P::VM>, mmtk: &'static MMTK<P::VM>) {
        let scheduler = worker.scheduler();
        let plan = mmtk.get_plan();

        scheduler.reset_state();

        // We are going to do sanity GC which will traverse the object graph again. Reset edge logger to clear recorded edges.
        #[cfg(feature = "extreme_assertions")]
        mmtk.edge_logger.reset();

        mmtk.sanity_begin(); // Stop & scan mutators (mutator scanning can happen before STW)

        // We use the cached roots for sanity gc, based on the assumption that
        // the stack scanning triggered by the selected plan is correct and precise.
        // FIXME(Wenyu,Tianle): When working on eager stack scanning on OpenJDK,
        // the stack scanning may be broken. Uncomment the following lines to
        // collect the roots again.
        // Also, remember to call `DerivedPointerTable::update_pointers(); DerivedPointerTable::clear();`
        // in openjdk binding before the second round of roots scanning.
        // for mutator in <P::VM as VMBinding>::VMActivePlan::mutators() {
        //     scheduler.work_buckets[WorkBucketStage::Prepare]
        //         .add(ScanMutatorRoots::<SanityGCProcessEdges<P::VM>>(mutator));
        // }
        {
            let mut sanity_checker = mmtk.sanity_checker.lock().unwrap();
            let root_edges = &sanity_checker.root_edges.clone();
            for roots in root_edges {
                scheduler.work_buckets[WorkBucketStage::Closure].add(
                    SanityGCProcessEdges::<P::VM>::new(
                        roots.clone(),
                        true,
                        mmtk,
                        WorkBucketStage::Closure,
                    ),
                );
            }
            for roots in root_edges {
                for root in roots {
                    sanity_checker.heapdump.roots.push(RootEdge {
                        objref: root.load().value() as u64,
                    });
                }
            }
            assert!(&sanity_checker.root_nodes.is_empty());
            for roots in &sanity_checker.root_nodes {
                scheduler.work_buckets[WorkBucketStage::Closure].add(ProcessRootNode::<
                    P::VM,
                    SanityGCProcessEdges<P::VM>,
                    SanityGCProcessEdges<P::VM>,
                >::new(
                    roots.clone(),
                    WorkBucketStage::Closure,
                ));
            }
        }
        // Prepare global/collectors/mutators
        worker.scheduler().work_buckets[WorkBucketStage::Prepare]
            .add(SanityPrepare::<P>::new(plan.downcast_ref::<P>().unwrap()));
        // Release global/collectors/mutators
        worker.scheduler().work_buckets[WorkBucketStage::Release]
            .add(SanityRelease::<P>::new(plan.downcast_ref::<P>().unwrap()));
    }
}

pub struct SanityPrepare<P: Plan> {
    pub plan: &'static P,
}

impl<P: Plan> SanityPrepare<P> {
    pub fn new(plan: &'static P) -> Self {
        Self { plan }
    }
}

impl<P: Plan> GCWork<P::VM> for SanityPrepare<P> {
    fn do_work(&mut self, _worker: &mut GCWorker<P::VM>, mmtk: &'static MMTK<P::VM>) {
        info!("Sanity GC prepare");
        let plan_mut: &mut P = unsafe { &mut *(self.plan as *const _ as *mut _) };
        {
            let mut sanity_checker = mmtk.sanity_checker.lock().unwrap();
            sanity_checker.refs.clear();
            if mmtk.is_in_harness() {
                sanity_checker
                    .iter
                    .epochs
                    .push(ShapesEpoch { shapes: vec![] });
            }
        }
        if plan_mut.constraints().needs_prepare_mutator {
            for mutator in <P::VM as VMBinding>::VMActivePlan::mutators() {
                mmtk.scheduler.work_buckets[WorkBucketStage::Prepare]
                    .add(PrepareMutator::<P::VM>::new(mutator));
            }
        }
        for w in &mmtk.scheduler.worker_group.workers_shared {
            let result = w.designated_work.push(Box::new(PrepareCollector));
            debug_assert!(result.is_ok());
        }
    }
}

pub struct SanityRelease<P: Plan> {
    pub plan: &'static P,
}

impl<P: Plan> SanityRelease<P> {
    pub fn new(plan: &'static P) -> Self {
        Self { plan }
    }
}

impl<P: Plan> GCWork<P::VM> for SanityRelease<P> {
    fn do_work(&mut self, _worker: &mut GCWorker<P::VM>, mmtk: &'static MMTK<P::VM>) {
        info!("Sanity GC release");
        {
            let mut sanity_checker = mmtk.sanity_checker.lock().unwrap();
            sanity_checker.clear_roots_cache();
            if mmtk.is_in_harness() {
                let gc_count = mmtk.stats.gc_count.load(atomic::Ordering::Relaxed);
                self.plan.for_each_space(&mut |s| {
                    let common = s.common();
                    assert!(
                        common.contiguous,
                        "Only support heapdump of contiguous spaces"
                    );
                    sanity_checker.heapdump.spaces.push(Space {
                        name: common.name.to_owned(),
                        start: common.start.as_usize() as u64,
                        end: (common.start + common.extent).as_usize() as u64,
                    })
                });
                sanity_checker
                    .heapdump
                    .dump_to_file(format!("heapdump.{}.binpb.zst", gc_count));
            }
            sanity_checker.heapdump.reset();
        }
        for mutator in <P::VM as VMBinding>::VMActivePlan::mutators() {
            mmtk.scheduler.work_buckets[WorkBucketStage::Release]
                .add(ReleaseMutator::<P::VM>::new(mutator));
        }
        for w in &mmtk.scheduler.worker_group.workers_shared {
            let result = w.designated_work.push(Box::new(ReleaseCollector));
            debug_assert!(result.is_ok());
        }
        mmtk.sanity_end();
    }
}

// #[derive(Default)]
pub struct SanityGCProcessEdges<VM: VMBinding> {
    base: ProcessEdgesBase<VM>,
}

impl<VM: VMBinding> Deref for SanityGCProcessEdges<VM> {
    type Target = ProcessEdgesBase<VM>;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<VM: VMBinding> DerefMut for SanityGCProcessEdges<VM> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl<VM: VMBinding> ProcessEdgesWork for SanityGCProcessEdges<VM> {
    type VM = VM;
    type ScanObjectsWorkType = ScanObjects<Self>;

    const OVERWRITE_REFERENCE: bool = false;
    fn new(
        edges: Vec<EdgeOf<Self>>,
        roots: bool,
        mmtk: &'static MMTK<VM>,
        bucket: WorkBucketStage,
    ) -> Self {
        Self {
            base: ProcessEdgesBase::new(edges, roots, mmtk, bucket),
            // ..Default::default()
        }
    }

    fn trace_object(&mut self, object: ObjectReference) -> ObjectReference {
        if object.is_null() {
            return object;
        }
        let mut sanity_checker = self.mmtk().sanity_checker.lock().unwrap();
        if !sanity_checker.refs.contains(&object) {
            // FIXME steveb consider VM-specific integrity check on reference.
            assert!(object.is_sane(), "Invalid reference {:?}", object);

            // Let plan check object
            assert!(
                self.mmtk().get_plan().sanity_check_object(object),
                "Invalid reference {:?}",
                object
            );

            // Let VM check object
            assert!(
                VM::VMObjectModel::is_object_sane(object),
                "Invalid reference {:?}",
                object
            );

            // Object is not "marked"
            sanity_checker.refs.insert(object); // "Mark" it
            trace!("Sanity mark object {}", object);
            self.nodes.enqueue(object);

            if self.mmtk().is_in_harness() {
                let mut edges: Vec<NormalEdge> = vec![];
                let mut objarray_length: Option<u64> = None;
                let mut instance_mirror_start: Option<u64> = None;
                let mut instance_mirror_count: Option<u64> = None;
                if <VM as VMBinding>::VMScanning::is_val_array(object) {
                    sanity_checker
                        .iter
                        .epochs
                        .last_mut()
                        .unwrap()
                        .shapes
                        .push(Shape {
                            kind: shape::Kind::ValArray as i32,
                            object: object.value() as u64,
                            offsets: vec![],
                        });
                } else if <VM as VMBinding>::VMScanning::is_obj_array(object) {
                    sanity_checker
                        .iter
                        .epochs
                        .last_mut()
                        .unwrap()
                        .shapes
                        .push(Shape {
                            kind: shape::Kind::ObjArray as i32,
                            object: object.value() as u64,
                            offsets: vec![],
                        });

                    <VM as VMBinding>::VMScanning::scan_object(
                        self.worker().tls,
                        object,
                        &mut |e: <VM as VMBinding>::VMEdge| {
                            edges.push(NormalEdge {
                                slot: e.as_address().as_usize() as u64,
                                objref: e.load().value() as u64,
                            });
                        },
                    );

                    objarray_length = Some(edges.len() as u64);
                } else {
                    if let Some((mirror_start, mirror_count)) = <VM as VMBinding>::VMScanning::instance_mirror_info(object) {
                        instance_mirror_start = Some(mirror_start);
                        instance_mirror_count = Some(mirror_count);
                    }
                    let mut s = vec![];
                    <VM as VMBinding>::VMScanning::scan_object(
                        self.worker().tls,
                        object,
                        &mut |e: <VM as VMBinding>::VMEdge| {
                            s.push(e.as_address().as_usize() as i64 - object.value() as i64);
                        },
                    );
                    // if s.len() > 512 {
                    //     <VM as VMBinding>::VMObjectModel::dump_object(object);
                    // }
                    sanity_checker
                        .iter
                        .epochs
                        .last_mut()
                        .unwrap()
                        .shapes
                        .push(Shape {
                            kind: shape::Kind::Scalar as i32,
                            object: object.value() as u64,
                            offsets: s,
                        });
                    <VM as VMBinding>::VMScanning::scan_object(
                        self.worker().tls,
                        object,
                        &mut |e: <VM as VMBinding>::VMEdge| {
                            edges.push(NormalEdge {
                                slot: e.as_address().as_usize() as u64,
                                objref: e.load().value() as u64,
                            });
                        },
                    );
                }
                sanity_checker.heapdump.objects.push(HeapObject {
                    start: object.value() as u64,
                    klass: <VM as VMBinding>::VMObjectModel::get_klass(object),
                    size: <VM as VMBinding>::VMObjectModel::get_current_size(object) as u64,
                    objarray_length,
                    instance_mirror_start,
                    instance_mirror_count,
                    edges: edges,
                })
            }
        }

        // If the valid object (VO) bit metadata is enabled, all live objects should have the VO
        // bit set when sanity GC starts.
        #[cfg(feature = "vo_bit")]
        if !crate::util::metadata::vo_bit::is_vo_bit_set::<VM>(object) {
            panic!("VO bit is not set: {}", object);
        }

        object
    }

    fn create_scan_work(&self, nodes: Vec<ObjectReference>) -> Self::ScanObjectsWorkType {
        ScanObjects::<Self>::new(nodes, false, WorkBucketStage::Closure)
    }
}
