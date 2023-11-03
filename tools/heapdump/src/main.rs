#[macro_use]
extern crate lazy_static;

use anyhow::Result;
use prost::Message;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{collections::HashMap, fs::File, io::Read};

include!(concat!(env!("OUT_DIR"), "/mmtk.util.sanity.rs"));

lazy_static! {
    static ref TIBS: Mutex<HashMap<u64, Arc<Tib>>> = Mutex::new(HashMap::new());
}

fn wrap_libc_call<T: PartialEq>(f: &dyn Fn() -> T, expect: T) -> Result<()> {
    let ret = f();
    if ret == expect {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

fn dzmmap_noreplace(start: u64, size: usize) -> Result<()> {
    let prot = libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC;
    let flags =
        libc::MAP_ANON | libc::MAP_PRIVATE | libc::MAP_FIXED_NOREPLACE | libc::MAP_NORESERVE;

    mmap_fixed(start, size, prot, flags)
}

fn mmap_fixed(start: u64, size: usize, prot: libc::c_int, flags: libc::c_int) -> Result<()> {
    let ptr = start as *mut libc::c_void;
    wrap_libc_call(
        &|| unsafe { libc::mmap(ptr, size, prot, flags, -1, 0) },
        ptr,
    )?;
    Ok(())
}

#[repr(C)]
#[derive(Debug)]
struct Tib {
    is_objarray: bool,
    oop_map_blocks: Vec<OopMapBlock>,
}

impl Tib {
    fn insert_with_cache(klass: u64, tib: impl FnOnce() -> Tib) -> Arc<Tib> {
        let mut tibs = TIBS.lock().unwrap();
        tibs.entry(klass).or_insert_with(|| Arc::new(tib()));
        tibs.get(&klass).unwrap().clone()
    }

    fn objarray(klass: u64) -> Arc<Tib> {
        Self::insert_with_cache(klass, || Tib {
            is_objarray: true,
            oop_map_blocks: vec![],
        })
    }

    fn non_objarray(klass: u64, obj: &HeapObject) -> Arc<Tib> {
        Self::insert_with_cache(klass, || {
            let mut oop_map_blocks: Vec<OopMapBlock> = vec![];
            for e in &obj.edges {
                if let Some(o) = oop_map_blocks.last_mut() {
                    if e.slot == obj.start + o.offset + o.count * 8 {
                        o.count += 1;
                        continue;
                    }
                }
                oop_map_blocks.push(OopMapBlock {
                    offset: e.slot - obj.start,
                    count: 1,
                });
            }

            Tib {
                is_objarray: false,
                oop_map_blocks,
            }
        })
    }
}

#[repr(C)]
#[derive(Debug)]
struct OopMapBlock {
    offset: u64,
    count: u64,
}

unsafe fn trace_object(o: u64) -> bool {
    // println!("Trace object: 0x{:x}", o as u64);
    if o == 0 {
        // skip null
        return false;
    }
    // Return false if already marked
    let mark_word = o as *mut u64;
    if *mark_word != 0 {
        false
    } else {
        *mark_word = 1;
        true
    }
}

unsafe fn scan_object(o: u64, mark_queue: &mut VecDeque<u64>) {
    let tib_ptr = *((o as *mut u64).wrapping_add(1) as *const *const Tib);
    if tib_ptr.is_null() {
        panic!("Object 0x{:x} has a null tib pointer", o as u64);
    }
    let tib: &Tib = &*tib_ptr;
    // println!("Tib: {:?}", tib);
    if tib.is_objarray {
        let objarray_length = *((o as *mut u64).wrapping_add(2) as *const u64);
        // println!("Objarray length: {}", objarray_length);
        for i in 0..objarray_length {
            let slot = (o as *mut u64).wrapping_add(3 + i as usize);
            mark_queue.push_back(*slot);
        }
    }
}

unsafe fn transitive_closure(roots: &[RootEdge]) {
    let start = Instant::now();
    // A queue of objref (possibly null)
    // aka node enqueuing
    let mut mark_queue: VecDeque<u64> = VecDeque::new();
    for root in roots {
        mark_queue.push_back(root.objref);
    }
    let mut marked_object: u64 = 0;
    while let Some(o) = mark_queue.pop_front() {
        if trace_object(o) {
            // not previously marked, now marked
            // now scan
            marked_object += 1;
            scan_object(o, &mut mark_queue);
        }
    }
    let elapsed = start.elapsed();
    println!(
        "Finished marking {} objects in {} ms",
        marked_object,
        elapsed.as_micros() as f64 / 1000f64
    );
}

fn main() -> Result<()> {
    let file = File::open("heapdump.20.binpb.zst")?;
    let mut reader = zstd::Decoder::new(file)?;
    let mut buf = vec![];
    reader.read_to_end(&mut buf)?;
    let heapdump = HeapDump::decode(buf.as_slice())?;
    for s in heapdump.spaces {
        println!("Mapping {} at 0x{:x}", s.name, s.start);
        dzmmap_noreplace(s.start, (s.end - s.start) as usize)?;
    }
    let start = Instant::now();
    for o in &heapdump.objects {
        unsafe {
            std::ptr::write::<u64>((o.start + 8) as *mut u64, o.start);
        }
        let tib = if o.is_objarray {
            Arc::as_ptr(&Tib::objarray(o.klass))
        } else {
            Arc::as_ptr(&Tib::non_objarray(o.klass, o))
        };
        // println!(
        //     "Object: 0x{:x}, Klass: 0x{:x}, TIB: 0x{:x}",
        //     o.start, o.klass, tib as u64
        // );
        unsafe {
            std::ptr::write::<u64>((o.start + 8) as *mut u64, tib as u64);
        }
        if o.is_objarray {
            unsafe {
                std::ptr::write::<u64>((o.start + 16) as *mut u64, o.objarray_length);
            }
        }
        for e in &o.edges {
            unsafe {
                std::ptr::write::<u64>(e.slot as *mut u64, e.objref);
            }
        }
    }
    let elapsed = start.elapsed();
    println!(
        "Finish deserializing the heapdump, {} objects in {} ms",
        heapdump.objects.len(),
        elapsed.as_micros() as f64 / 1000f64
    );
    unsafe {
        transitive_closure(&heapdump.roots);
    }
    Ok(())
}
