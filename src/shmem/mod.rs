mod shmem_bindgen;
use bincode;
use serde::{Deserialize, Serialize};
use shmem_bindgen::*;

use crate::VppApiTransport;

use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct GlobalState {
    created: bool,
    receive_buffer: Vec<u8>,
}

lazy_static! {
    static ref GLOBAL: Arc<Mutex<GlobalState>> = {
        let mut gs = GlobalState {
            ..Default::default()
        };

        Arc::new(Mutex::new(gs))
    };
}

#[derive(Serialize, Deserialize, Debug)]
struct SockMsgHeader {
    _q: u64,
    msglen: u32,
    gc_mark: u32,
}

#[no_mangle]
pub unsafe extern "C" fn shmem_default_cb(raw_data: *const u8, len: i32) {
    let data_slice = unsafe { std::slice::from_raw_parts(raw_data, len as usize) };
    let mut gs = GLOBAL.lock().unwrap();

    let hdr = SockMsgHeader {
        _q: 0,
        msglen: data_slice.len() as u32,
        gc_mark: 0,
    };
    let hs = bincode::serialize(&hdr).unwrap();
    gs.receive_buffer.extend_from_slice(&hs);
    gs.receive_buffer.extend_from_slice(data_slice);

    println!("Got {} bytes of data", len);
}

#[no_mangle]
pub unsafe extern "C" fn vac_error_handler(arg: *const u8, msg: *const u8, msg_len: i32) {
    println!("Error: {} bytes of message", msg_len);
}

pub struct Transport {
    connected: bool,
}

impl Transport {
    pub fn new() -> Self {
        let mut gs = GLOBAL.lock().unwrap();
        if gs.created {
            panic!("One transport already created!");
        }

        gs.created = true;

        unsafe { vac_mem_init(0) };
        Transport { connected: false }
    }
}

impl std::io::Write for Transport {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let wr_len = buf.len();
        let err = unsafe { vac_write(buf.as_ptr(), wr_len as i32) };
        Ok(wr_len)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        // no-op
        Ok(())
    }
}

impl VppApiTransport for Transport {
    fn connect(&mut self, name: &str, chroot_prefix: Option<&str>, rx_qlen: i32) -> i32 {
        use std::ffi::CString;

        let name_c = CString::new(name).unwrap();
        let chroot_prefix_c = chroot_prefix.map(|x| CString::new(x).unwrap());

        let name_arg = name_c.as_ptr();
        let chroot_prefix_arg = if let Some(p) = chroot_prefix_c {
            p.as_ptr()
        } else {
            std::ptr::null_mut()
        };
        let err =
            unsafe { vac_connect(name_arg, chroot_prefix_arg, Some(shmem_default_cb), rx_qlen) };
        if err == 0 {
            self.connected = true;
        }
        println!("Returning {}", err);
        return err;
    }
    fn disconnect(&mut self) {
        if self.connected {
            let err = unsafe { vac_disconnect() };
            self.connected = false;
        }
    }
    fn get_msg_index(&mut self, name: &str) -> u16 {
        0
    }
    fn get_table_max_index(&mut self) -> u16 {
        0
    }
    fn ping(&mut self) -> bool {
        use std::io::Write;
        self.write(b"\x02\x4d234556789b123456789c123456789d123");
        true
    }
    fn dump(&self) {
        let mut gs = GLOBAL.lock().unwrap();
        println!("Global state: {:?}", &gs);
    }
}
