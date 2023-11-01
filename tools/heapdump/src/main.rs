#[macro_use]
extern crate lazy_static;

use anyhow::Result;
use prost::Message;
use std::ptr;
use std::{collections::HashMap, fs::File, io::Read};
use std::sync::{Mutex, Arc};

include!(concat!(env!("OUT_DIR"), "/mmtk.util.sanity.rs"));

lazy_static! {
    static ref TIBS: Mutex<HashMap<u64, Arc<TIB>>> = Mutex::new(HashMap::new());
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
struct TIB {
    is_objarray: bool,
    size: u64,
    oop_map_blocks: Vec<OopMapBlock>,
}

impl TIB {
    fn no_ref(klass: u64, size: u64) -> Arc<TIB> {
        // if TIBS
        let mut tibs = TIBS.lock().unwrap();
        if !tibs.contains_key(&klass) {
            tibs.insert(klass, Arc::new(TIB { is_objarray: false, size, oop_map_blocks: vec![]}));
        }
        tibs.get(&klass).unwrap().clone()
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
    for o in heapdump.objects {
        unsafe {
            std::ptr::write::<u64>((o.start + 8) as *mut u64, o.start);
        }
        let mut tib = ptr::null();
        if !o.is_objarray {
            if o.edges.len() == 0 {
                tib = Arc::as_ptr(&TIB::no_ref(o.klass, o.size));
            }
        }
        // println!("Klass: 0x{:x}, TIB: 0x{:x}", o.klass, tib as u64);
        unsafe { std::ptr::write::<u64>((o.start + 8) as *mut u64, tib as u64); }
        if o.is_objarray {
            unsafe {
                std::ptr::write::<u64>((o.start + 16) as *mut u64, o.objarray_length);
            }
        }
        for e in o.edges {
            unsafe {
                std::ptr::write::<u64>(e.slot as *mut u64, e.objref);
            }
        }
    }
    println!("Finish deserializing the heapdump");

    Ok(())
}
