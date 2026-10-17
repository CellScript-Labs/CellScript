//! Counts guest pages touched by successful VM memory operations, including
//! ELF/argv initialization and syscall buffers. This is not host heap/RSS.
use ckb_vm::{memory::Memory, Bytes, Error, SparseMemory, WXorXMemory, RISCV_PAGESIZE};
use std::collections::BTreeSet;

type Inner = WXorXMemory<SparseMemory<u64>>;
pub struct ObservedMemory {
    inner: Inner,
    pages: BTreeSet<u64>,
}
impl ObservedMemory {
    fn touched(&mut self, address: u64, length: u64) {
        if length != 0 {
            let end = address.checked_add(length - 1).expect("successful memory access cannot wrap");
            self.pages.extend(address / RISCV_PAGESIZE as u64..=end / RISCV_PAGESIZE as u64);
        }
    }
    pub fn touched_page_bytes(&self) -> u64 {
        self.pages.len() as u64 * RISCV_PAGESIZE as u64
    }
}

macro_rules! load {
    ($name:ident, $width:expr) => {
        fn $name(&mut self, address: &u64) -> Result<u64, Error> {
            let value = self.inner.$name(address)?;
            self.touched(*address, $width);
            Ok(value)
        }
    };
}
macro_rules! store {
    ($name:ident, $width:expr) => {
        fn $name(&mut self, address: &u64, value: &u64) -> Result<(), Error> {
            self.inner.$name(address, value)?;
            self.touched(*address, $width);
            Ok(())
        }
    };
}
impl Memory for ObservedMemory {
    type REG = u64;
    fn new() -> Self {
        Self { inner: Inner::new(), pages: BTreeSet::new() }
    }
    fn new_with_memory(size: usize) -> Self {
        Self { inner: Inner::new_with_memory(size), pages: BTreeSet::new() }
    }
    fn init_pages(&mut self, address: u64, size: u64, flags: u8, source: Option<Bytes>, offset: u64) -> Result<(), Error> {
        self.inner.init_pages(address, size, flags, source, offset)?;
        self.touched(address, size);
        Ok(())
    }
    fn fetch_flag(&mut self, page: u64) -> Result<u8, Error> {
        self.inner.fetch_flag(page)
    }
    fn set_flag(&mut self, page: u64, flag: u8) -> Result<(), Error> {
        self.inner.set_flag(page, flag)
    }
    fn clear_flag(&mut self, page: u64, flag: u8) -> Result<(), Error> {
        self.inner.clear_flag(page, flag)
    }
    fn memory_size(&self) -> usize {
        self.inner.memory_size()
    }
    fn store_byte(&mut self, address: u64, size: u64, value: u8) -> Result<(), Error> {
        self.inner.store_byte(address, size, value)?;
        self.touched(address, size);
        Ok(())
    }
    fn store_bytes(&mut self, address: u64, value: &[u8]) -> Result<(), Error> {
        self.inner.store_bytes(address, value)?;
        self.touched(address, value.len() as u64);
        Ok(())
    }
    fn load_bytes(&mut self, address: u64, size: u64) -> Result<Bytes, Error> {
        let value = self.inner.load_bytes(address, size)?;
        self.touched(address, size);
        Ok(value)
    }
    fn execute_load16(&mut self, address: u64) -> Result<u16, Error> {
        let value = self.inner.execute_load16(address)?;
        self.touched(address, 2);
        Ok(value)
    }
    fn execute_load32(&mut self, address: u64) -> Result<u32, Error> {
        let value = self.inner.execute_load32(address)?;
        self.touched(address, 4);
        Ok(value)
    }
    load!(load8, 1);
    load!(load16, 2);
    load!(load32, 4);
    load!(load64, 8);
    store!(store8, 1);
    store!(store16, 2);
    store!(store32, 4);
    store!(store64, 8);
    fn lr(&self) -> &u64 {
        self.inner.lr()
    }
    fn set_lr(&mut self, value: &u64) {
        self.inner.set_lr(value);
    }
}

#[test]
fn guest_page_measurement_counts_crossings_initialization_and_repeated_access_once() {
    let mut memory = ObservedMemory::new();
    assert_eq!(memory.touched_page_bytes(), 0);
    memory.store_bytes(4095, &[1, 2]).unwrap();
    assert_eq!(memory.touched_page_bytes(), 8192);
    memory.load64(&0).unwrap();
    assert_eq!(memory.touched_page_bytes(), 8192);
    memory.init_pages(8192, 4096, 0, None, 0).unwrap();
    assert_eq!(memory.touched_page_bytes(), 12288);
    memory.load_bytes(1, 0).unwrap();
    assert_eq!(memory.touched_page_bytes(), 12288);
    assert!(memory.load_bytes(memory.memory_size() as u64, 1).is_err());
    assert_eq!(memory.touched_page_bytes(), 12288);
}
