#[macro_use]
extern crate lazy_static;

use anyhow::Result;
use prost::Message;
use std::sync::{Arc, Mutex};
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
        Self::insert_with_cache(klass, || {
            Tib {
                is_objarray: true,
                oop_map_blocks: vec![],
            }
        })
    }

    fn non_objarray(klass: u64, obj: &HeapObject) -> Arc<Tib> {
        Self::insert_with_cache(klass, || {
            let mut oop_map_blocks: Vec<OopMapBlock> = vec![];
            for e in &obj.edges {
                if let Some(o) = oop_map_blocks.last_mut() {
                    if e.slot == o.offset + o.count * 8 {
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
                is_objarray: true,
                oop_map_blocks,
            }
        })
    }
}

#[repr(C)]
struct OopMapBlock {
    offset: u64,
    count: u64,
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
    for o in &heapdump.objects {
        unsafe {
            std::ptr::write::<u64>((o.start + 8) as *mut u64, o.start);
        }
        let tib = if o.is_objarray {
            Arc::as_ptr(&Tib::objarray(o.klass))
        } else {
            Arc::as_ptr(&Tib::non_objarray(o.klass, o))
        };
        // println!("Klass: 0x{:x}, TIB: 0x{:x}", o.klass, tib as u64);
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
    println!("Finish deserializing the heapdump, {} objects", heapdump.objects.len());

    Ok(())
}
